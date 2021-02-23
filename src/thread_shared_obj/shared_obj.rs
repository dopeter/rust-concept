
#[derive(Debug,Clone)]
pub struct SharedStringObj {
    name:String
}

impl SharedStringObj {
    pub fn new(name:String) -> SharedStringObj {
        SharedStringObj {name:name}
    }
}

pub fn new(name:String) -> SharedStringObj {
    SharedStringObj {name:name}
}

// impl Copy for SharedObj{
//
// }
//
// impl Clone for SharedObj{
//     fn clone(&self) -> Self {
//         SharedObj::new(self.name.clone())
//     }
// }

#[derive(Debug,Copy, Clone)]
pub struct SharedNumObj{
    age:u8
}

impl SharedNumObj{
    pub fn new(age:u8) ->SharedNumObj{
        SharedNumObj{age: age}
    }
}

#[derive(Debug,Copy, Clone)]
pub struct SharedPointerObj{
    pointer:SharedNumObj
}

impl SharedPointerObj{
    pub fn new(pointer:SharedNumObj) -> SharedPointerObj{
        SharedPointerObj{pointer }
    }
}

#[derive(Debug,Clone)]
pub struct ShardPointerObj2{
    pointer:SharedStringObj
}

impl ShardPointerObj2{
    pub fn new (pointer:SharedStringObj) -> ShardPointerObj2{
        ShardPointerObj2{pointer }
    }
}

// impl Clone for ShardPointerObj2{
//     fn clone(&self) -> Self {
//         ShardPointerObj2{pointer:self.pointer.clone()}
//     }
// }

#[derive(Debug,Copy,Clone)]
pub struct CopyableObj{
    num1:i64,
    num2:u64
}

impl CopyableObj{
    pub fn new(num1:i64,num2:u64) -> CopyableObj{
        CopyableObj{num1,num2}
    }
}

#[derive(Debug)]
pub struct UncopiableObj{
    str1:String
}
impl Clone for UncopiableObj{
    fn clone(&self) -> Self {
        UncopiableObj{str1: "hahah".to_string() }
    }
}

// #[derive(Debug,Clone)]
// pub struct UncopiableObj{
//     str1:String
// }

impl UncopiableObj{
    pub fn new(str1:String) -> UncopiableObj{
        UncopiableObj{str1}
    }
}

