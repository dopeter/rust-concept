use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use crate::tikv_batch::test_runner::{Message, Handler, Runner, Builder};
use crate::tikv_batch::mpsc::unbounded;
use crate::tikv_batch::batch::create_system;
use crate::tikv_batch::config::Config;
use crossbeam::channel::{SendError, TrySendError,TryRecvError,RecvTimeoutError};
use crate::tikv_batch::mailbox::BasicMailbox;
use std::time::Duration;

fn counter_closure(counter: &Arc<AtomicUsize>) -> Message {
    let c = counter.clone();
    Message::Callback(Box::new(move |_: &Handler, _: &mut Runner| {
        c.fetch_add(1, Ordering::SeqCst);
    }))
}

fn noop() -> Message { Message::Callback(Box::new(|_, _| ())) }

fn unreachable() -> Message {
    Message::Callback(Box::new(|_: &Handler, _: &mut Runner| unreachable!()))
}

#[test]
fn test_basic() {
    let (ctrl_tx, mut ctrl_fsm) = Runner::new(10);
    let (ctrl_drop_tx, ctrl_drop_rx) = unbounded();
    ctrl_fsm.sender = Some(ctrl_drop_tx);

    let (router, mut system) = create_system(&Config::default(), ctrl_tx, ctrl_fsm);

    let builder = Builder::new();
    system.spawn("test".to_owned(), builder);

    // Missing mailbox should report error.
    match router.force_send(1, unreachable()) {
        Err(SendError(_)) => {}
        Ok(_) => panic!("send should fail")
    }

    match router.send(1, unreachable()) {
        Err(TrySendError::Disconnected(_)) => {}
        Ok(_) => panic!("send should fail"),
        Err(TrySendError::Full(_)) => panic!("expect disconnected."),
    }

    let (tx, rx) = unbounded();
    let router_ = router.clone();
    //Control mailbox should be connected.
    router.send_control(Message::Callback(Box::new(
        move |_: &Handler, _: &mut Runner| {
            let (sender, mut runner) = Runner::new(10);
            let (tx1, rx1) = unbounded();
            runner.sender = Some(tx1);

            let mailbox = BasicMailbox::new(sender, runner, Arc::default());
            router_.register(1, mailbox);
            tx.send(rx1).unwrap();
        }
    ))).unwrap();
    let runner_drop_rx = rx.recv_timeout(Duration::from_secs(3)).unwrap();

    // Registered mailbox should be connected.
    router.force_send(1, noop()).unwrap();
    router.send(1, noop()).unwrap();

    // Send should respect capacity limit, while force_send not.
    let (tx, rx) = unbounded();
    router.send(1,
                Message::Callback(Box::new(move |_: &Handler, _: &mut Runner| {
                    rx.recv_timeout(Duration::from_secs(100)).unwrap();
                }))).unwrap();

    let counter = Arc::default();
    let send_cnt = (0..)
        .take_while(|_| router.send(1, counter_closure(&counter)).is_ok())
        .count();
    match router.send(1, counter_closure(&counter)) {
        Err(TrySendError::Full(_)) => {}
        Err(TrySendError::Disconnected(_)) => panic!("mailbox should still be connected."),
        Ok(_) => panic!("send should fail"),
    }

    router.force_send(1, counter_closure(&counter)).unwrap();
    tx.send(1).unwrap();

    // Flush
    let (tx, rx) = unbounded();
    router.force_send(1,
                      Message::Callback(Box::new(move |_: &Handler, _: &mut Runner| {
                          tx.send(1).unwrap();
                      }))).unwrap();
    rx.recv_timeout(Duration::from_secs(100)).unwrap();

    let c = counter.load(Ordering::SeqCst);
    assert_eq!(c, send_cnt + 1);

    // Close should release resources.
    assert_eq!(runner_drop_rx.try_recv(), Err(TryRecvError::Empty));
    router.close(1);
    assert_eq!(
        runner_drop_rx.recv_timeout(Duration::from_secs(3)),
        Err(RecvTimeoutError::Disconnected)
    );
    match router.send(1,unreachable()){
        Err(TrySendError::Disconnected(_)) => {},
        Ok(_) => panic!("send should fail."),
        Err(TrySendError::Full(_)) => panic!("sender should be closed."),
    }
    match router.force_send(1,unreachable()){
        Err(SendError(_)) =>(),
        Ok(_) => panic!("send should fail.")
    }
    assert_eq!(ctrl_drop_rx.try_recv(),Err(TryRecvError::Empty));
    system.shutdown();
    assert_eq!(
        ctrl_drop_rx.recv_timeout(Duration::from_secs(3)),
        Err(RecvTimeoutError::Disconnected)
    )











}