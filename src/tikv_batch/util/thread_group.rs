use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::cell::RefCell;

#[derive(Default)]
struct GroupPropertiesInner{
    shutdown: AtomicBool
}

#[derive(Default,Clone)]
pub struct GroupProperties{
    inner: Arc<GroupPropertiesInner>
}

impl GroupProperties{
    #[inline]
    pub fn mark_shutdown(&self){
        self.inner.shutdown.store(true,Ordering::SeqCst);
    }
}

thread_local! {
    static PROPERTIES: RefCell<Option<GroupProperties>> = RefCell::new(None);
}

pub fn current_properties() -> Option<GroupProperties>{
    PROPERTIES.with(|p|p.borrow().clone())
}

pub fn set_properties(props:Option<GroupProperties>){
    PROPERTIES.with(move |p|{p.replace(props);})
}