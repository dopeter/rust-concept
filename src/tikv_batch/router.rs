use crate::tikv_batch::fsm::{Fsm, FsmScheduler};
use std::sync::{Mutex, Arc};
use std::collections::HashMap;
use crate::tikv_batch::mailbox::BasicMailbox;
use std::sync::atomic::{AtomicUsize, AtomicBool};
use std::cell::Cell;
use crate::tikv_batch::util::lru::LruCache;

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
    state_cnt: Atc<AtomicUsize>,

    // Indicates the router is shutdown down or not.
    shutdown: Arc<AtomicBool>,
}

impl<N, C, Ns, Cs> Router<N, C, Ns, Cs>
    where N: Fsm, C: Fsm, Ns: FsmScheduler<Fsm=N> + Clone, Cs: FsmScheduler<Fsm=C> + Clone {

    pub(super) fn new(
        control_box:BasicMailbox<C>,
        normal_scheduler:Ns,
        control_scheduler:Cs,
        state_cnt:Arc<AtomicUsize>
    )-> Router<N,C,Ns,Cs>{

        Router{
            normals:Arc::new(Mutex::new(NormalMailMap{
                map:HashMap::default(),
                alive_cnt:Arc::default(),
            })),
            caches:Cell::new(LruCache::with_capacity_and_sample(1024,7)),
            control_box,
            normal_scheduler,
            control_scheduler,
            state_cnt,
            shutdown:Arc::new(AtomicBool::new(false))
        }

    }

}



