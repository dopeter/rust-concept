
#[derive(Debug,Copy,Clone)]
pub struct OwnerTest{
    num_1:u64,
    num_2:u64
}

impl OwnerTest{
    fn new(num_1:u64,num_2:u64) -> Self{
        OwnerTest{num_1,num_2}
    }
}


pub fn test_struct_owner(){

    let owner_test_1 =OwnerTest::new(1, 2);

    transfer_owner(owner_test_1);

    println!("Original struct----------");
    println!("{:#?}",owner_test_1);

}

pub fn transfer_owner(mut struct_test:OwnerTest){
    struct_test.num_1=3;
    println!("{:#?}",struct_test);
}

#[cfg(test)]
mod tests {
    use crate::owner::struct_owner::{transfer_owner, test_struct_owner};

    #[test]
    fn test_val_struct_owner() {
        test_struct_owner()
    }


}


