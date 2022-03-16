use std::sync::{Arc, Mutex};
use crate::tikv_batch::fsm::{Priority, Fsm};
use crate::tikv_batch::mpsc::{Receiver, Sender, LooseBoundedSender, loose_bounded};
use crate::tikv_batch::mailbox::BasicMailbox;
use std::borrow::Cow;
use crate::tikv_batch::batch::{PollHandler, HandlerBuilder};
use derive_more::{Add, AddAssign};


//region Runner
pub struct Runner{
    is_stopped:bool,
    recv:Receiver<Message>,
    mailbox:Option<BasicMailbox<Runner>>,
    pub sender:Option<Sender<()>>,
    /// Result of the calculation triggered by `Message::Loop`.
    /// Stores it inside `Runner` to avoid accidental optimization.
    res:usize,
    priority:Priority
}

impl Fsm for Runner{
    type Message = Message;

    fn is_stopped(&self) -> bool {
        self.is_stopped
    }

    fn set_mailbox(&mut self, _mailbox: Cow<'_, BasicMailbox<Self>>) where Self: Sized {
        self.mailbox=Some(_mailbox.into_owned());
    }

    fn take_mailbox(&mut self) -> Option<BasicMailbox<Self>> where Self: Sized {
        self.mailbox.take()
    }

    fn get_priority(&self) -> Priority {
        self.priority
    }
}

impl Runner{
    pub fn new(cap:usize) -> (LooseBoundedSender<Message>,Box<Runner>){
        let (tx,rx)=loose_bounded(cap);
        let fsm=Box::new(
            Runner{
                is_stopped:false,
                recv:rx,
                mailbox:None,
                sender:None,
                res:0,
                priority:Priority::Normal,
            }
        );

        (tx,fsm)
    }

    pub fn set_priority(&mut self,priority:Priority){
        self.priority=priority;
    }
}

//endregion
#[derive(Add, PartialEq, Debug, Default, AddAssign, Clone, Copy)]
pub struct HandleMetrics{
    pub begin:usize,
    pub control:usize,
    pub normal:usize
}

//region Handler
pub struct Handler{
    local:HandleMetrics,
    metrics:Arc<Mutex<HandleMetrics>>,
    priority:Priority
}

impl Handler{
    fn handle(&mut self,r:&mut Runner) ->Option<usize>{
        for _ in 0..16{
            match r.recv.try_recv(){
                Ok(Message::Loop(count))=>{
                    for _ in 0..count{
                        // Some calculation to represent a CPU consuming work
                        r.res *=count;
                        r.res %=count+1;
                    }
                }
                Ok(Message::Callback(cb))=> cb(self,r),
                Err(_) => break,
            }
        }
        Some(0)
    }
}

impl PollHandler<Runner,Runner> for Handler{
    fn begin(&mut self, batch_size: usize) {
        self.local.begin+=1;
    }

    fn handle_control(&mut self, control: &mut Runner) -> Option<usize> {
        self.local.control+=1;
        self.handle(control)
    }

    fn handle_normal(&mut self, normal: &mut Runner) -> Option<usize> {
        self.local.normal+=1;
        self.handle(normal)
    }

    fn end(&mut self, batch: &mut [Box<Runner>]) {
        let mut c=self.metrics.lock().unwrap();
        *c+=self.local;
        self.local=HandleMetrics::default();
    }
}

//endregion

pub struct Builder{
    pub metrics:Arc<Mutex<HandleMetrics>>,
}

impl Builder{
    pub fn new()-> Builder{
        Builder{
            metrics:Arc::default()
        }
    }
}

impl HandlerBuilder<Runner,Runner> for Builder{
    type Handler = Handler;

    fn build(&mut self, priority: Priority) -> Self::Handler {
        Handler{
            local:HandleMetrics::default(),
            metrics:self.metrics.clone(),
            priority
        }
    }
}

pub enum Message{

    Loop(usize),
    Callback(Box<dyn FnOnce(&Handler,&mut Runner) + Send + 'static>),

}