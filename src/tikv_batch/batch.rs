use std::time::Instant;
use crate::tikv_batch::fsm::Fsm;
use std::borrow::Cow;

enum FsmTypes<N,C>{
    Normal(Box<N>),
    Control(Box<C>),
    /// Used as a signal that scheduler should be shutdown.
    Empty
}

//region Batch

pub struct Batch<N,C>{
    normals:Vec<Box<N>>,
    timers:Vec<Instant>,
    control:Option<Box<C>>,
}

impl<N:Fsm,C:Fsm> Batch<N,C>{

    pub fn with_capacity(cap:usize) -> Btach<N,C>{
        Btach{
            normals:Vec::with_capacity(cap),
            timers:Vec::with_capacity(cap),
            control:None,
        }
    }

    fn push(&mut self,fsm:FsmTypes<N,C>) ->bool{
        match fsm {
            FsmTypes::Normal(n) =>{
                self.normals.push(n);
                self.timers.push(Instant::now())
            }
            FsmTypes::Control(c)=>{
                assert!(self.control.is_none());
                self.control=Some(c);
            }
            FsmTypes::Empty=> false
        }
        true
    }

    fn is_empty(&self) -> bool{
        self.normals.is_empty() && self.control.is_none()
    }

    fn clear(&mut self){
        self.normals.clear();
        self.timers.clear();
        self.control.take();
    }

    pub fn release(&mut self,index:usize,checked_len:usize){
        let mut fsm = self.normals.swap_remove(index);
        let mailbox=fsm.take_mailbox().unwrap();
        mailbox.release(fsm);

        if mailbox.len()==checked_len{
            self.timers.swap_remove(index);
        }else{
            match mailbox.take_fsm() {
                None=>{},
                Some(mut s)=>{
                    s.set_mailbox(Cow::Owned(mailbox));
                    let last_index=self.normals.len();
                    self.normals.push(s);
                    self.normals.swap(index,last_index);
                }
            }
        }

    }

    pub fn remove(&mut self,index:usize){

        let mut fsm=self.normals.swap_remove(index);
        let mailbox=fsm.take_mailbox().unwrap();
        if mailbox.is_empty(){
            mailbox.release(fsm);
            self.timers.swap_remove(index);
        }else{
            fsm.set_mailbox(Cow::Owned(mailbox));
            let last_index=self.normals.len();
            self.normals.push(fsm);
            self.normals.swap(index,last_index);
        }

    }

    pub fn reschedule(&mut self,router:&BatchRouter<N,C>, index:usize){

    }

}

//endregion

pub type BatchRouter<N,C> =Router<N,C,>