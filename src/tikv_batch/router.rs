use crate::tikv_batch::fsm::{Fsm, FsmScheduler, FsmState};
use std::sync::{Mutex, Arc};
use std::collections::HashMap;
use crate::tikv_batch::mailbox::{BasicMailbox, Mailbox};
use std::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
use std::cell::Cell;
use crate::tikv_batch::util::lru::LruCache;
use crate::tikv_batch::util::Either;
use std::sync::mpsc::{TrySendError, SendError};
use std::mem;

/// A struct that traces the approximate memory usage of router.
#[derive(Default)]
pub struct RouterTrace {
    pub alive: usize,
    pub leak: usize,
}

struct NormalMailMap<N: Fsm> {
    map: HashMap<u64, BasicMailbox<N>>,
    //Count of Mailboxes that is stored in `map`.
    alive_cnt: Arc<AtomicUsize>,
}

enum CheckDoResut<T> {
    NotExist,
    Invalid,
    Valid(T),
}


/// Router route messages to its target mailbox.
///
/// Every fsm has a mailbox, hence it's necessary to have an address book
/// that can deliver messages to specified fsm, which is exact router.
///
/// In our abstract model, every batch system has two different kind of
/// fsms. First is normal fsm, which does the common work like peers in a
/// raftstore model or apply delegate in apply model. Second is control fsm,
/// which does some work that requires a global view of resources or creates
/// missing fsm for specified address. Normal fsm and control fsm can have
/// different scheduler, but this is not required.
///
pub struct Router<N: Fsm, C: Fsm, Ns, Cs> {
    normals: Arc<Mutex<NormalMailMap<N>>>,
    caches: Cell<LruCache<u64, BasicMailbox<N>>>,
    pub(super) control_box: BasicMailbox<C>,
    pub(crate) normal_scheduler: Ns,
    control_scheduler: Cs,

    // Count of Mailboxes that is not destroyed.
    // Added when a Mailbox created, and subtracted it when a Mailbox destroyed.
    state_cnt: Arc<AtomicUsize>,

    // Indicates the router is shutdown down or not.
    shutdown: Arc<AtomicBool>,
}

impl<N, C, Ns, Cs> Router<N, C, Ns, Cs>
    where N: Fsm, C: Fsm, Ns: FsmScheduler<Fsm=N> + Clone, Cs: FsmScheduler<Fsm=C> + Clone {
    pub(super) fn new(
        control_box: BasicMailbox<C>,
        normal_scheduler: Ns,
        control_scheduler: Cs,
        state_cnt: Arc<AtomicUsize>,
    ) -> Router<N, C, Ns, Cs> {
        Router {
            normals: Arc::new(Mutex::new(NormalMailMap {
                map: HashMap::default(),
                alive_cnt: Arc::default(),
            })),
            caches: Cell::new(LruCache::with_capacity_and_sample(1024, 7)),
            control_box,
            normal_scheduler,
            control_scheduler,
            state_cnt,
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }

    #[inline]
    fn check_do<F, R>(&self, addr: u64, mut f: F) -> CheckDoResut<R>
        where F: FnMut(&BasicMailbox<N>) -> Option<R> {

        let caches=unsafe{&mut *self.caches.as_ptr()};

        let mut connected=true;
        if let Some(mailbox) =caches.get(&addr){
            match f(mailbox){
                Some(r) => return CheckDoResut::Valid(r),
                None=>{connected=false;}
            }
        }

        let (cnt,mailbox) ={
            let mut boxes=self.normals.lock().unwrap();
            let cnt=boxes.map.len();
            let b=match boxes.map.get_mut(&addr) {
                Some(mailbox) =>mailbox.clone(),
                None=>{
                    drop(boxes);
                    if !connected{
                        caches.remove(&addr);
                    }
                    return CheckDoResut::NotExist;
                }
            };
            (cnt,b)
        };
        if cnt>caches.capacity() || cnt<caches.capacity()/2{
            caches.resize(cnt);
        }

        let res=f(&mailbox);
        match res {
            Some(r) =>{
                caches.insert(addr,mailbox);
                CheckDoResut::Valid(r)
            }
            None =>{
                if !connected{
                    caches.remove(&addr);
                }
                CheckDoResut::Invalid
            }
        }

    }

    /// Register a mailbox with given address.
    pub fn register(&self,addr:u64,mailbox:BasicMailbox<N>){
        let mut normals=self.normals.lock().unwrap();
        if let Some(mailbox) =normals.map.insert(addr,mailbox){
            mailbox.close();
        }
        normals.alive_cnt.store(normals.map.len(),Ordering::Relaxed);
    }

    pub fn register_all(&self,mailboxes:Vec<(u64,BasicMailbox<N>)>){
        let mut normals=self.normals.lock().unwrap();
        normals.map.reserve(mailboxes.len());
        for(addr,mailbox) in mailboxes{
            if let Some(m) =normals.map.insert(addr,mailbox){
                m.close();
            }
        }
    }

    /// Get the mailbox of specified address.
    pub fn mailbox(&self,addr:u64) -> Option<Mailbox<N,Ns>>{
        let res=self.check_do(addr,|mailbox|{
           if mailbox.is_connected(){
               Some(Mailbox::new(mailbox.clone(),self.normal_scheduler.clone()))
           }else{
               None
           }
        });

        match res {
            CheckDoResut::Valid(r) =>Some(r),
            _=>None
        }
    }

    /// Get the mailbox of control fsm.
    pub fn control_mailbox(&self) -> Mailbox<C,Cs>{
        Mailbox::new(self.control_box.clone(),self.control_scheduler.clone())
    }

    /// Try to send a message to specified address.
    /// If Either::Left is returned, then the message is sent.
    /// Otherwise, it indicates mailbox is not found.
    #[inline]
    pub fn try_send(
        &self,
        addr:u64,
        msg:N::Message
    ) -> Either<Result<(),TrySendError<N::Message>>,N::Message>{
        let mut msg=Some(msg);
        let res=self.check_do(addr,|mailbox|{
            let m=msg.take().unwrap();
            match mailbox.try_send(m,&self.normal_scheduler){
                Ok(())=>Some(Ok(())),
                r@Err(TrySendError::Full(_))=>{
                    Some(r)
                }
                Err(TrySendError::Disconnected(m))=>{
                    msg=Some(m);
                    None
                }
            }
        });

        match res {
            CheckDoResut::Valid(r) =>Either::Left(r),
            CheckDoResut::Invalid=>Either::Left(Err(TrySendError::Disconnected(msg.unwrap()))),
            CheckDoResut::NotExist=>Either::Right(msg.unwrap())
        }

    }

    #[inline]
    pub fn send(&self,addr:u64,msg:N::Message) -> Result<(),TrySendError<N::Message>>{
        match self.try_send(addr,msg) {
            Either::Left(res) =>res,
            Either::Right(m) => Err(TrySendError::Disconnected(m))
        }
    }

    #[inline]
    pub fn force_send(&self,addr:u64,msg:N::Message) ->Result<(),SendError<N::Message>>{
        match self.send(addr,msg) {
            Ok(())=>Ok(()),
            Err(TrySendError::Full(m))=>{
                let caches=unsafe{&mut *self.caches.as_ptr()};
                caches.get(&addr).unwrap().force_send(m,&self.normal_scheduler)
            }
            Err(TrySendError::Disconnected(m))=>{
                if self.is_shutdown(){Ok(())}
                else{Err(SendError(m))}
            }
        }
    }

    #[inline]
    pub fn send_control(&self,msg:C::Message)->Result<(),TrySendError<C::Message>>{
        match self.control_box.try_send(msg,&self.control_scheduler) {
            Ok(())=>Ok(()),
            r @ Err(TrySendError::Full(_))=>{
                r
            }
            r=>r
        }
    }

    pub fn broadcast_normal(&self,mut msg_gen:impl FnMut()->N::Message){
        let mailboxes=self.normals.lock().unwrap();
        for mailbox in mailboxes.map.values(){
            let _ = mailbox.force_send(msg_gen(),&self.normal_scheduler);
        }
    }

    pub fn broadcast_shutdown(&self){
        self.shutdown.store(true,Ordering::SeqCst);
        unsafe {&mut *self.caches.as_ptr()}.clear();
        let mut mailboxes=self.normals.lock().unwrap();
        for (addr,mailbox) in mailboxes.map.drain(){
            mailbox.close();
        }
        self.control_box.close();
        self.normal_scheduler.shutdown();
        self.control_scheduler.shutdown();
    }

    pub fn close(&self,addr:u64){
        unsafe {&mut *self.caches.as_ptr()}.remove(&addr);
        let mut mailboxes=self.normals.lock().unwrap();
        if let Some(mb) = mailboxes.map.remove(&addr){
            mb.close();
        }

        mailboxes.alive_cnt.store(mailboxes.map.len(),Ordering::Relaxed);
    }

    pub fn clear_cache(&self) {
        unsafe {&mut *self.caches.as_ptr()}.clear();
    }

    pub fn state_cnt(&self) -> &Arc<AtomicUsize>{
        &self.state_cnt
    }

    pub fn alive_cnt(&self) -> Arc<AtomicUsize>{
        self.normals.lock().unwrap().alive_cnt.clone()
    }

    pub fn trace(&self) -> RouterTrace{
        let alive=self.normals.lock().unwrap().alive_cnt.clone();
        let total=self.state_cnt.load(Ordering::Relaxed);
        let alive=alive.load(Ordering::Relaxed);
        // 1 represents the control fsm.
        let leak=if total>alive+1{
            total-alive-1
        }else{0};

        let mailbox_unit=mem::size_of::<(u64,BasicMailbox<N>)>();
        let state_unit=mem::size_of::<FsmState<N>>();
        // Every message in crossbeam sender needs 8 bytes to store state.
        let message_unit=mem::size_of::<N::Message>()+8;
        // crossbeam unbounded channel sender has a list of blocks. Every block has 31 unit
        // and every sender has at least one sender.
        let sender_block_unit=31;
        RouterTrace{
            alive:(mailbox_unit * 8 /7
            +state_unit+message_unit+sender_block_unit)
            *alive,
            leak:(state_unit+message_unit*sender_block_unit)*leak
        }
    }

}

impl<N:Fsm,C:Fsm,Ns:Clone,Cs:Clone> Clone for Router<N,C,Ns,Cs>{
    fn clone(&self) -> Self {
        Router{
            normals:self.normals.clone(),
            caches:Cell::new(LruCache::with_capacity_and_sample(1024,7)),
            control_box:self.control_box.clone(),
            normal_scheduler:self.normal_scheduler.clone(),
            control_scheduler:self.control_scheduler.clone(),
            shutdown:self.shutdown.clone(),
            state_cnt:self.state_cnt.clone()
        }
    }
}



