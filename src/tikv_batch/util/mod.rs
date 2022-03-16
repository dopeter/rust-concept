use std::thread;

pub mod lru;
pub mod thread_group;


#[derive(Debug,Clone)]
pub enum Either<L,R>{
    Left(L),
    Right(R),
}


pub fn get_tag_from_thread_name() -> Option<String> {
    thread::current()
        .name()
        .and_then(|name| name.split("::").skip(1).last())
        .map(From::from)
}

/// Makes a thread name with an additional tag inherited from the current thread.
#[macro_export]
macro_rules! thd_name {
    ($name:expr) => {{
        crate::tikv_batch::util::get_tag_from_thread_name()
            .map(|tag| format!("{}::{}", $name, tag))
            .unwrap_or_else(|| $name.to_owned())
    }};
}