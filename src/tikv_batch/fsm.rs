use std::sync::atomic::{AtomicUsize, AtomicPtr, Ordering};
use std::sync::Arc;
use crate::tikv_batch::mailbox::BasicMailbox;
use std::borrow::Cow;
use std::ptr;

// The fsm is notified
const NOTIFY_STATE_NOTIFIED:usize=0;

// THe fsm is idel.
const NOTIFY_STATE_IDLE:usize=1;

//The fsm is expected to be dropped.
const NOTIFY_STATE_DROP:usize=2;

#[derive(Clone,Copy,Debug,Eq, PartialEq)]
pub enum Priority{
    Low,
    Normal,
}

pub trait Fsm{

    type  Message:Send;

    fn is_stopped(&self) ->bool;

    /// Set a mailbox to Fsm , which should be used to send message to itself.
    fn set_mailbox(&mut self, _mailbox:Cow<'_,BasicMailbox<Self>>)
    where Self:Sized,
    {}

    fn take_mailbox(&mut self) -> Option<BasicMailbox<Self>>
    where Self:Sized,
    {None}

    fn get_priority(&self) -> Priority
    {Priority::Normal}

}

//region FsmState

pub struct FsmState<N>{
    status:AtomicUsize,
    data:AtomicPtr<N>,
    state_cnt:Arc<AtomicUsize>,
}

impl<N:Fsm> FsmState<N>{

    pub fn new(data:Box<N>,state_cnt:Arc<AtomicUsize>) -> FsmState<N>{
        state_cnt.fetch_add(1,Ordering::Relaxed);
        FsmState{
            status:AtomicUsize::new(NOTIFY_STATE_IDLE),
            data:AtomicPtr::new(Box::into_raw(data)),
            state_cnt
        }
    }

    /// Take the fsm if it's IDLE.
    pub fn take_fsm(&self) -> Option<Box<N>>{
        let res=self.status.compare_exchange(
            NOTIFY_STATE_IDLE,
            NOTIFY_STATE_NOTIFIED,
            Ordering::AcqRel,
            Ordering::Acquire
        );

        if res.is_err(){
            None
        }

        let p=self.data.swap(ptr::null_mut(),Ordering::AcqRel);
        if !p.is_null(){
            Some(unsafe {Box::from_raw(p)})
        }else{
            panic!("inconsistent status and data,something should be wrong.")
        }

    }

    pub fn notify<S:FsmScheduler<Fsm=N>>(
        &self,
        scheduler:&S,
        mailbox:Cow<'_,BasicMailbox<N>>
    ){
        match self.take_fsm() {
            None=>{}
            Some(mut n)=>{
                n.set_mailbox(mailbox);
                scheduler.schedule(n);
            }
        }
    }

    pub fn release(&self,fsm:Box<N>){
        let previous=self.data.swap(Box::into_raw(fsm),Ordering::AcqRel);

        let mut previous_status=NOTIFY_STATE_NOTIFIED;

        if previous.is_null(){
            let res=self.status.compare_exchange(
                NOTIFY_STATE_NOTIFIED,
                NOTIFY_STATE_IDLE,
                Ordering::AcqRel,
                Ordering::Acquire
            );

            previous_status = match res {
                Ok(_) => return,
                Err(NOTIFY_STATE_DROP) =>{
                    let ptr=self.data.swap(ptr::null_mut(),Ordering::AcqRel);
                    unsafe{Box::from_raw(ptr)}
                    return;
                }
                Err(s) => s,
            }
        }

        panic!("")
    }

    #[inline]
    pub fn clear(&self){
        match self.status.swap(NOTIFY_STATE_DROP,Ordering::AcqRel) {
            NOTIFY_STATE_NOTIFIED | NOTIFY_STATE_DROP =>return,
            _=>{}
        }

        let ptr=self.data.swap(ptr::null_mut(),Ordering::SeqCst);
        if !ptr.is_null(){
            unsafe {Box::from_raw(ptr);}
        }
    }


}

impl<N> Drop for FsmState<N>{
    fn drop(&mut self) {
        let ptr=self.data.swap(ptr::null_mut(),Ordering::SeqCst);
        if !ptr.is_null(){
            unsafe {Box::from_raw(ptr)}
        }

        self.state_cnt.fetch_sub(1,Ordering::Relaxed);
    }
}

//endregion

//region FsmScheduler

pub trait FsmScheduler{
    type Fsm:Fsm;

    ///Schedule a Fsm for later handles.
    fn schedule(&self,fsm:Box<Self::Fsm>);

    ///Shudown the scheduler, which indicates that resources like
    /// background thread pool should be released.
    fn shutdown(&self);
}

//endregion