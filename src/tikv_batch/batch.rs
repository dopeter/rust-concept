use std::time::{Instant, Duration};
use crate::tikv_batch::fsm::{Fsm, FsmScheduler, Priority};
use std::borrow::{Cow, Borrow, BorrowMut};
use crate::tikv_batch::mpsc::{Sender, LooseBoundedSender, Receiver};
use crate::tikv_batch::router::Router;
use crate::tikv_batch::config::Config;
use crate::tikv_batch::mailbox::BasicMailbox;
use std::ops::Deref;
use std::thread::JoinHandle;
use crossbeam::channel::{self, SendError};
use crate::tikv_batch::util;
use std::thread;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

enum FsmTypes<N, C> {
    Normal(Box<N>),
    Control(Box<C>),
    /// Used as a signal that scheduler should be shutdown.
    Empty,
}

/// Makes a thread name with an additional tag inherited from the current thread.
macro_rules! thd_name {
    ($name:expr) => {{
        $crate::get_tag_from_thread_name()
            .map(|tag| format!("{}::{}", $name, tag))
            .unwrap_or_else(|| $name.to_owned())
    }};
}

//region NormalScheduler
pub struct NormalScheduler<N, C> {
    sender: channel::Sender<FsmTypes<N, C>>,
    low_sender: channel::Sender<FsmTypes<N, C>>,
}

impl<N, C> Clone for NormalScheduler<N, C> {
    #[inline]
    fn clone(&self) -> Self {
        NormalScheduler {
            sender: self.sender.clone(),
            low_sender: self.low_sender.clone(),
        }
    }
}

impl<N, C> FsmScheduler for NormalScheduler<N, C>
    where N: Fsm {
    type Fsm = N;

    fn schedule(&self, fsm: Box<Self::Fsm>) {
        let sender = match fsm.get_priority() {
            Priority::Normal => &self.sender,
            Priority::Low => &self.low_sender,
        };

        match sender.send(FsmTypes::Normal(fsm)) {
            Ok(()) => {}
            Err(channel::SendError(FsmTypes::Normal(fsm))) => {}
            _ => {}
        }
    }

    fn shutdown(&self) {
        for _ in 0..100 {
            let _ = self.sender.send(FsmTypes::Empty);
            let _ = self.low_sender.send(FsmTypes::Empty);
        }
    }
}


//endregion

//region ControlScheduler
pub struct ControlScheduler<N, C> {
    sender: channel::Sender<FsmTypes<N, C>>,
    low_sender: channel::Sender<FsmTypes<N, C>>,
}

impl<N, C> Clone for ControlScheduler<N, C> {
    #[inline]
    fn clone(&self) -> Self {
        ControlScheduler {
            sender: self.sender.clone(),
            low_sender: self.low_sender.clone(),
        }
    }
}

impl<N, C> FsmScheduler for ControlScheduler<N, C>
    where N: Fsm {
    type Fsm = N;

    fn schedule(&self, fsm: Box<Self::Fsm>) {
        let sender = match fsm.get_priority() {
            Priority::Normal => &self.sender,
            Priority::Low => &self.low_sender,
        };

        match sender.send(FsmTypes::Normal(fsm)) {
            Ok(()) => {}
            Err(channel::SendError(FsmTypes::Normal(fsm))) => {}
            _ => {}
        }
    }

    fn shutdown(&self) {
        for _ in 0..100 {
            let _ = self.sender.send(FsmTypes::Empty);
            let _ = self.low_sender.send(FsmTypes::Empty);
        }
    }
}
//endregion

//region scheduler macro
// A macro to introduce common definition of scheduler.
// macro_rules! impl_sched {
//     ($name:ident,$ty:path,Fsm=$fsm:tt) =>{
//         pub struct $name<N,C>{
//             sender:Sender<FsmTypes<N,C>>,
//             low_sender:Sender<FsmTypes<N,C>>
//         }
//
//         impl<N,C> Clone for $name<N,C>{
//             #[inline]
//             fn clone(&self) -> $name<N,C>{
//                 $name{
//                     sender:self.sender.clone(),
//                     low_sender:self.low_sender.clone(),
//                 }
//             }
//         }
//
//         impl<N,C> FsmScheduler for $name<N,C>
//         where $fsm:Fsm{
//
//             type Fsm=$fsm;
//
//             #[inline]
//             pub fn schedule(&self,fsm:Box<Self::Fsm>){
//                 let sender=match fsm.get_priority(){
//                     Priority::Normal =>&self.sender,
//                     Priority::Low => &self.low_sender,
//                 };
//
//                 match sender.send($ty(fsm)){
//                     Ok(())=>{}
//                     Err(SendError($ty(fsm))) -> {}
//                     _ => {}
//                 }
//             }
//
//             pub fn shutdown(&self){
//                 for _ in 0..100{
//                     let _ =self.sender.send(FsmTypes::Empty);
//                     let _ =self.low_sender.send(FsmTypes::Empty);
//                 }
//             }
//
//
//         }
//
//     };
// }
//
// impl_sched!(NormalScheduler,FsmTypes::Normal,Fsm=N);
// impl_sched!(ControlScheduler,FsmTypes::Control,Fsm=C);
//endregion


//region Batch

pub struct Batch<N, C> {
    normals: Vec<Box<N>>,
    timers: Vec<Instant>,
    control: Option<Box<C>>,
}

impl<N: Fsm, C: Fsm> Batch<N, C> {
    pub fn with_capacity(cap: usize) -> Batch<N, C> {
        Batch {
            normals: Vec::with_capacity(cap),
            timers: Vec::with_capacity(cap),
            control: None,
        }
    }

    fn push(&mut self, fsm: FsmTypes<N, C>) -> bool {
        match fsm {
            FsmTypes::Normal(n) => {
                self.normals.push(n);
                self.timers.push(Instant::now())
            }
            FsmTypes::Control(c) => {
                assert!(self.control.is_none());
                self.control = Some(c);
            }
            FsmTypes::Empty => false
        }
        true
    }

    fn is_empty(&self) -> bool {
        self.normals.is_empty() && self.control.is_none()
    }

    fn clear(&mut self) {
        self.normals.clear();
        self.timers.clear();
        self.control.take();
    }

    pub fn release(&mut self, index: usize, checked_len: usize) {
        let mut fsm = self.normals.swap_remove(index);
        let mailbox = fsm.take_mailbox().unwrap();
        mailbox.release(fsm);

        if mailbox.len() == checked_len {
            self.timers.swap_remove(index);
        } else {
            match mailbox.take_fsm() {
                None => {}
                Some(mut s) => {
                    s.set_mailbox(Cow::Owned(mailbox));
                    let last_index = self.normals.len();
                    self.normals.push(s);
                    self.normals.swap(index, last_index);
                }
            }
        }
    }

    pub fn remove(&mut self, index: usize) {
        let mut fsm = self.normals.swap_remove(index);
        let mailbox = fsm.take_mailbox().unwrap();
        if mailbox.is_empty() {
            mailbox.release(fsm);
            self.timers.swap_remove(index);
        } else {
            fsm.set_mailbox(Cow::Owned(mailbox));
            let last_index = self.normals.len();
            self.normals.push(fsm);
            self.normals.swap(index, last_index);
        }
    }

    pub fn reschedule(&mut self, router: &BatchRouter<N, C>, index: usize) {
        let fsm = self.normals.swap_remove(index);
        self.timers.swap_remove(index);
        router.normal_scheduler.schedule(fsm);
    }

    pub fn release_control(&mut self, control_box: &BasicMailbox<C>, checked_len: usize) -> bool {
        let s = self.control.take().unwrap();
        control_box.release(s);
        if control_box.len() == checked_len {
            true
        } else {
            match control_box.take_fsm() {
                None => true,
                Some(s) => {
                    self.control = Some(s);
                    false
                }
            }
        }
    }

    pub fn remove_control(&mut self, control_box: &BasicMailbox<C>) {
        if control_box.is_empty() {
            let s = self.control.take().unwrap();
            control_box.release(s);
        }
    }
}

//endregion

//region Poller && PollHandler
/// A handler that poll all FSM in ready.
pub trait PollHandler<N, C> {
    /// This function is called at the very beginning of every round.
    fn begin(&mut self, batch_size: usize);

    /// This function is called when handling readiness for control FSM.
    fn handle_control(&mut self, control: &mut C) -> Option<usize>;

    /// This function is called when handling readiness for normal FSM.
    fn handle_normal(&mut self, normal: &mut N) -> Option<usize>;

    /// This function is called at the end of every round.
    fn end(&mut self, batch: &mut [Box<N>]);

    fn pause(&mut self) {}

    fn get_priority(&self) -> Priority { Priority::Normal }
}

struct Poller<N: Fsm, C: Fsm, Handler> {
    router: Router<N, C, NormalScheduler<N, C>, ControlScheduler<N, C>>,
    fsm_receiver: channel::Receiver<FsmTypes<N, C>>,
    handler: Handler,
    max_batch_size: usize,
    reschedule_duration: Duration,
}

enum ReschedulePolicy {
    Release(usize),
    Remove,
    Schedule,
}

impl<N: Fsm, C: Fsm, Handler: PollHandler<N, C>> Poller<N, C, Handler> {
    fn fetch_fsm(&mut self, batch: &mut Batch<N, C>) -> bool {
        if batch.control.is_some() {
            return true;
        }

        if let Ok(fsm) = self.fsm_receiver.try_recv() {
            return batch.push(fsm);
        }

        if batch.is_empty() {
            self.handler.pause();
            if let Ok(fsm) = self.fsm_receiver.recv() {
                return batch.push(fsm);
            }
        }

        !batch.is_empty()
    }

    /// Poll for readiness and forward to handler. Remove stale peer if necessary.
    fn poll(&mut self) {
        let mut batch = Batch::with_capacity(self.max_batch_size);
        let mut reschedule_fsms = Vec::with_capacity(self.max_batch_size);

        let mut run = true;
        while run && self.fetch_fsm(&mut batch) {
            let max_batch_size = std::cmp::max(self.max_batch_size, batch.normals.len());
            self.handler.begin(max_batch_size);

            if batch.control.is_some() {
                let len = self.handler.handle_control(batch.control.as_mut().unwrap());
                if batch.control.as_ref().unwrap().is_stopped() {
                    batch.remove_control(&self.router.control_box);
                } else if let Some(len) = len {
                    batch.release_control(&self.router.control_box, len);
                }
            }

            let mut hot_fsm_count = 0;
            for (i, p) in batch.normals.iter_mut().enumerate() {
                let len = self.handler.handle_normal(p);
                if p.is_stopped() {
                    reschedule_fsms.push((i, ReschedulePolicy::Remove));
                } else if p.get_priority() != self.handler.get_priority() {
                    reschedule_fsms.push((i, ReschedulePolicy::Schedule));
                } else {
                    if batch.timers[i].elapsed() >= self.reschedule_duration {
                        hot_fsm_count += 1;

                        if hot_fsm_count % 2 == 0 {
                            reschedule_fsms.push((i, ReschedulePolicy::Schedule));
                            continue;
                        }
                    }

                    if let Some(l) = len {
                        reschedule_fsms.push((i, ReschedulePolicy::Release(l)));
                    }
                }
            }

            let mut fsm_cnt = batch.normals.len();
            while batch.normals.len() < max_batch_size {
                if let Ok(fsm) = self.fsm_receiver.try_recv() {
                    run = batch.push(fsm);
                }

                if !run || fsm_cnt >= batch.normals.len() { break; }

                let len = self.handler.handle_normal(&mut batch.normals[fsm_cnt]);

                if batch.normals[fsm_cnt].is_stopped() {
                    reschedule_fsms.push((fsm_cnt, ReschedulePolicy::Remove));
                } else if let Some(l) = len {
                    reschedule_fsms.push((fsm_cnt, ReschedulePolicy::Release(l)));
                }

                fsm_cnt += 1;
            }

            self.handler.end(&mut batch.normals);
        }
        batch.clear();
    }
}

//endregion

/// A builder trait that can build up poll handlers.
pub trait HandlerBuilder<N, C> {
    type Handler: PollHandler<N, C>;

    fn build(&mut self, priority: Priority) -> Self::Handler;
}


pub struct BatchSystem<N: Fsm, C: Fsm> {
    name_prefix: Option<String>,
    router: BatchRouter<N, C>,
    receiver: channel::Receiver<FsmTypes<N, C>>,
    low_receiver: channel::Receiver<FsmTypes<N, C>>,
    pool_size: usize,
    max_batch_size: usize,
    workers: Vec<JoinHandle<()>>,
    reschedule_duration: Duration,
    low_priority_pool_size: usize,
}

impl<N, C> BatchSystem<N, C>
    where N: Fsm + Send + 'static, C: Fsm + Send + 'static {
    pub fn router(&self) -> &BatchRouter<N, C> { &self.router }

    fn start_poller<B>(&mut self, name: String, priority: Priority, builder: &mut B)
        where B: HandlerBuilder<N, C>, B::Handler: Send + 'static {
        let handler = builder.build(priority);
        let receiver = match priority {
            Priority::Normal => self.receiver.clone(),
            Priority::Low => self.low_receiver.clone()
        };

        let mut poller = Poller {
            router: self.router.clone(),
            fsm_receiver: receiver,
            handler,
            max_batch_size: self.max_batch_size,
            reschedule_duration: self.reschedule_duration,
        };

        let props = util::thread_group::current_properties();

        let t = thread::Builder::new()
            .name(name)
            .spawn(move || {
                util::thread_group::set_properties(props);
                // set_io_type(IOType::ForegroundWrite);
                poller.poll();
            })
            .unwrap();

        self.workers.push(t);
    }


    pub fn spawn<B>(&mut self, name_prefix: String, mut builder: B)
        where B: HandlerBuilder<N, C>, B::Handler: Send + 'static {

        for i in 0..self.pool_size{
            self.start_poller(
                thd_name!(format!("{}-{}",name_prefix,i)),
                Priority::Normal,
                &mut builder
            );
        }

        for i in 0..self.low_priority_pool_size{
            self.start_poller(
                thd_name!(format!("{}-low-{}",name_prefix,i)),
                Priority::Low,
                &mut builder
            );
        }

        self.name_prefix=Some(name_prefix);

    }

    pub fn shutdown(&mut self){
        if self.name_prefix.is_none(){return;}

        let name_prefix=self.name_prefix.take().unwrap();
        println!("shutdown batch system {}",name_prefix);
        self.router.broadcast_shutdown();

        let mut last_error=None;
        for h in self.workers.drain(..){
            println!("waiting for {}",h.thread().name().unwrap());
            if let Err(e)=h.join(){
                println!("failed to join worker thread: {:?}",e);
                last_error=Some(e);
            }
        }

        if let Some(e)=last_error{
            panic!("failed to join worker thread: {:?}",e);
        }
        println!("batch system {} is stopped.",name_prefix);

    }

}

pub type BatchRouter<N, C> = Router<N, C, NormalScheduler<N, C>, ControlScheduler<N, C>>;

/// Create a batch system with the given thread name prefix and pool size.
/// `sender` and `controller` should be paired.
pub fn create_system<N: Fsm, C: Fsm>(
    cfg: &Config,
    sender: LooseBoundedSender<C::Message>,
    controller: Box<C>,
) -> (BatchRouter<N, C>, BatchSystem<N, C>) {
    let state_cnt=Arc::new(AtomicUsize::new(0));

    let control_box=BasicMailbox::new(sender,controller,state_cnt.clone());

    let (tx,rx)=channel::unbounded();
    let(tx2,rx2)=channel::unbounded();

    let normal_scheduler=NormalScheduler{
        sender:tx.clone(),
        low_sender:tx2.clone()
    };

    let control_scheduler=ControlScheduler{
        sender:tx,
        low_sender:tx2
    };

    let router=Router::new(control_box,normal_scheduler,control_scheduler,state_cnt);

    let system=BatchSystem{
        name_prefix:None,
        router:router.clone(),
        receiver:rx,
        low_receiver:rx2,
        pool_size:cfg.pool_size,
        max_batch_size:cfg.max_batch_size(),
        reschedule_duration:cfg.reschedule_duration,
        workers:vec![],
        low_priority_pool_size:cfg.low_priority_pool_size
    };
    (router,system)

}