use crate::tikv_batch::fsm::{Fsm, FsmState, FsmScheduler};
use std::sync::Arc;
use crate::tikv_batch::mpsc::LooseBoundedSender;
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc::{SendError, TrySendError};
use std::borrow::Cow;

//region BasicMailbox
pub struct BasicMailbox<Owner: Fsm> {
    sender: LooseBoundedSender<Owner::Message>,
    state: Arc<FsmState<Owner>>,
}

impl<Owner: Fsm> BasicMailbox<Owner> {
    #[inline]
    pub fn new(sender: LooseBoundedSender<Owner::Message>,
               fsm: Box<Owner>,
               state_cnt: Arc<AtomicUsize>,
    ) -> BasicMailbox<Owner> {
        BasicMailbox {
            sender,
            state: Arc::new(FsmState::new(fsm, state_cnt)),
        }
    }

    pub(crate) fn is_connected(&self) -> bool {
        self.sender.is_sender_connected()
    }

    pub(crate) fn release(&self, fsm: Box<Owner>) {
        self.state.release(fsm)
    }

    pub(crate) fn take_fsm(&self) -> Option<Box<Owner>> {
        self.state.take_fsm()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.sender.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.sender.is_empty()
    }


    #[inline]
    pub fn force_send<S: FsmScheduler<Fsm=Owner>>(
        &self,
        msg: Owner::Message,
        scheduler: &S,
    ) -> Result<(), SendError<Owner::Message>> {
        self.sender.force_send(msg)?;
        self.state.notify(scheduler, Cow::Borrowed(self));
        Ok(())
    }

    #[inline]
    pub fn try_send<S: FsmScheduler<Fsm=Owner>>(
        &self,
        msg: Owner::Message,
        scheduler: &S,
    ) -> Result<(), TrySendError<Owner::Message>> {
        self.sender.try_send(msg)?;
        self.state.notify(scheduler, Cow::Borrowed(self));
        Ok(())
    }

    #[inline]
    pub(crate) fn close(&self) {
        self.sender.close_sender();
        self.state.clear()
    }
}

impl<Owner: Fsm> Clone for BasicMailbox<Owner> {
    #[inline]
    fn clone(&self) -> BasicMailbox<Owner> {
        BasicMailbox {
            sender: self.sender.clone(),
            state: self.state.clone(),
        }
    }
}

//endregion

//region Mailbox
/// A more high level mailbox
pub struct Mailbox<Owner, Scheduler>
    where
        Owner: Fsm,
        Scheduler: FsmSchduler<Fsm=Owner> {
    mailbox: BasicMailbox<Owner>,
    scheduler: Scheduler,
}

impl<Owner, Scheduler> Mailbox<Owner, Scheduler>
    where Owner: Fsm, Scheduler: FsmScheduler<Fsm=Owner> {

    pub fn new(mailbox:BasicMailbox<Owner>,scheduler:Scheduler) -> Mailbox<Owner,Scheduler>{
        Mailbox{mailbox,scheduler}
    }

    #[inline]
    pub fn force_send(&self,msg:Owner::Message) -> Result<(),SendError<Owner::Message>>{
        self.mailbox.force_send(msg,&self.scheduler)
    }

    #[inline]
    pub fn try_send(&self,msg:Owner::Message) -> Result<(),TrySendError<Owner::Message>>{
        self.mailbox.try_send(msg,&self.scheduler)
    }

}

//endregion