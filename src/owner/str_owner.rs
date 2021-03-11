

pub fn test_str_owner(){

    let mut name="name".to_string();
    {

    }


    // name.push_str("acc");
    // println!("{:#?}",name);


    let name_ptr=name.as_str();
    println!("{:#?}",name_ptr);

    let mut name_ptr2=&mut name;
    println!("{:#?}",name_ptr2);
    name_ptr2.push_str("ddd");

}


#[cfg(test)]
mod tests {
    use crate::owner::str_owner::test_str_owner;

    #[test]
    fn test_val() {
        test_str_owner()
    }


}


