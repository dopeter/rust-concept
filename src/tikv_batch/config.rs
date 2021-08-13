use std::time::Duration;

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
pub struct Config{
    pub max_batch_size:Option<usize>,
    pub pool_size:usize,
    pub reschedule_duration:Duration,
    pub low_priority_pool_size:usize
}

impl Config{
    pub fn max_batch_size(&self) -> usize{
        self.max_batch_size.unwrap_or(256)
    }
}

impl Default for Config{
    fn default() -> Self {
        Config{
            max_batch_size:None,
            pool_size:2,
            reschedule_duration:Duration::from_secs(5),
            low_priority_pool_size:1
        }
    }
}