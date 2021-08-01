use crate::tikv_batch::fsm::Fsm;
use std::sync::{Mutex, Arc};
use std::collections::HashMap;
use crate::tikv_batch::mailbox::BasicMailbox;
use std::sync::atomic::AtomicUsize;
use std::cell::Cell;

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
pub struct Router<N:Fsm,C:Fsm,Ns,Cs>{
    normals:Arc<Mutex<NormalMailMap<N>>>,
    caches:Cell<>
}



struct NormalMailMap<N:Fsm>{
    map:HashMap<u64,BasicMailbox<N>>,
    //Count of Mailboxes that is stored in `map`.
    alive_cnt:Arc<AtomicUsize>
}

enum CheckDoResut<T>{
    NotExist,
    Invalid,
    Valid(T)
}