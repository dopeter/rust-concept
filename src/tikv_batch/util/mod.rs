pub mod lru;
pub mod thread_group;


#[derive(Debug,Clone)]
pub enum Either<L,R>{
    Left(L),
    Right(R),
}