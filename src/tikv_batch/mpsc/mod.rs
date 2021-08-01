use std::cell::Cell;
use std::sync::Arc;
use crossbeam::channel;
use std::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
use crossbeam::channel::{
    RecvError, RecvTimeoutError, SendError, TryRecvError, TrySendError,
};
use std::time::Duration;

const CHECK_INTERVAL: usize = 8;

//region LooseBoundedSender
pub struct LooseBoundedSender<T> {
    sender: Sender<T>,
    tried_cnt: Cell<usize>,
    limit: usize,
}

impl<T> LooseBoundedSender<T> {
    #[inline]
    pub fn len(&self) -> usize {
        self.sender.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.sender.is_empty()
    }

    #[inline]
    pub fn force_send(&self, t: T) -> Result<(), SendError<T>> {
        let cnt = self.tried_cnt.get();
        self.tried_cnt.set(cnt + 1);

        self.sender.send(t)
    }

    #[inline]
    pub fn try_send(&self, t: T) -> Result<(), TrySendError<T>> {
        let cnt = self.tried_cnt.get();
        if cnt < CHECK_INTERVAL {
            self.tried_cnt.set(cnt + 1);
        } else if self.len() < self.limit {
            self.tried_cnt.set(1);
        } else {
            return Err(TrySendError::Full(t));
        }

        match self.sender.send(t) {
            Ok(()) => Ok(()),
            Err(SendError(t)) => Err(TrySendError::Disconnected(t)),
        }
    }

    #[inline]
    pub fn close_sender(&self) {
        self.sender.close_sender();
    }

    #[inline]
    pub fn is_sender_connected(&self) -> bool {
        self.sender.state.is_sender_connected()
    }
}

impl<T> Clone for LooseBoundedSender<T> {
    #[inline]
    fn clone(&self) -> Self {
        LooseBoundedSender {
            sender: self.sender.clone(),
            tried_cnt: self.tried_cnt.clone(),
            limit: self.limit,
        }
    }
}


//endregion

#[inline]
pub fn unbounded<T>() -> (Sender<T>, Receiver<T>) {
    let state = Arc::new(State::new());
    let (sender, receiver) = channel::unbounded();
    (
        Sender {
            sender,
            state: state.clone(),
        },
        Receiver(receiver, state),
    )
}

#[inline]
pub fn bounded<T>(cap: usize) -> (Sender<T>, Receiver<T>) {
    let state = Arc::new(State::new());
    let (sender, receiver) = channel::bounded(cap);
    (
        Sender {
            sender,
            state: state.clone(),
        },
        Receiver(receiver, state),
    )
}

pub fn loose_bounded<T>(cap: usize) -> (LooseBoundedSender<T>, Receiver<T>) {
    let (sender, receiver) = unbounded();
    (
        LooseBoundedSender {
            sender,
            tried_cnt: Cell::new(0),
            limit: cap,
        },
        receiver,
    )
}


//region State
pub struct State {
    sender_cnt: AtomicUsize,
    connected: AtomicBool,
}

impl State {
    fn new() -> State {
        State {
            sender_cnt: AtomicUsize::new(1),
            connected: AtomicBool::new(true),
        }
    }

    #[inline]
    fn is_sender_connected(&self) -> bool { self.connected.load(Ordering::Acquire) }
}

//endregion


//region Sender
pub struct Sender<T> {
    sender: channel::Sender<T>,
    state: Arc<State>,
}

impl<T> Sender<T> {
    #[inline]
    pub fn len(&self) -> usize { self.sender.len() }

    #[inline]
    pub fn is_empty(&self) -> bool { self.sender.is_empty() }

    /// Blocks current thread until a message is sent or the channel is disconnected.
    #[inline]
    pub fn send(&self, t: T) -> Result<(), SendError<T>> {
        if self.state.is_sender_connected() {
            self.sender.send(t)
        } else {
            Err(SendError(t))
        }
    }


    /// Attempts to send a message into the channel without blocking.
    #[inline]
    pub fn try_send(&self, t: T) -> Result<(), TrySendError<T>> {
        if self.state.is_sender_connected() {
            self.sender.try_send(t)
        } else {
            Err(TrySendError::Disconnected(t))
        }
    }

    /// Set state to disconnected , stop sending any message.
    #[inline]
    pub fn close_sender(&self) {
        self.state.connected.store(false, Ordering::Release);
    }

    #[inline]
    pub fn is_sender_connected(&self) -> bool {
        self.state.is_sender_connected()
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.state.sender_cnt.fetch_add(1, Ordering::AcqRel);
        Sender {
            sender: self.sender.clone(),
            state: self.state.clone(),
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        if self.state.sender_cnt.fetch_add(-1, Ordering::AcqRel) == 1 {
            self.close_sender();
        }
    }
}
//endregion

//region Receiver
pub struct Receiver<T> {
    receiver: channel::Receiver<T>,
    state: Arc<State>,
}

impl<T> Receiver<T> {
    #[inline]
    pub fn len(&self) -> usize {
        self.receiver.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.receiver.is_empty()
    }

    /// blocking receive message
    #[inline]
    pub fn recv(&self) -> Result<T, RecvError> {
        self.receiver.recv()
    }

    /// receive message without blocking
    #[inline]
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.receiver.try_recv()
    }

    /// receive message with timeout
    #[inline]
    pub fn recv_timeout(&self, timeout: Duration) -> Result<T, RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }
}

impl<T> Drop for Receiver<T> {
    #[inline]
    fn drop(&mut self) {
        self.state.connected.store(flase, Ordering::Release);
    }
}


//endregion