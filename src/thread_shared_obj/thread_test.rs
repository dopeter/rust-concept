use crate::thread_shared_obj::shared_obj::*;


pub fn test_thread_share() {
    let duration = std::time::Duration::from_millis(3000);
    let main_duration = std::time::Duration::from_millis(20000);

    let shared_string_obj = SharedStringObj::new("shared_obj_1".to_string());
    let shared_string_obj_clone = shared_string_obj.clone();

    let shared_num_obj = SharedNumObj::new(21);

    let test_num = 5;

    let num_obj = SharedNumObj::new(37);
    let shared_pointer_obj = SharedPointerObj::new(num_obj);

    let string_obj = SharedStringObj::new("String_test".to_string());
    let shared_pointer_obj_2 = ShardPointerObj2::new(string_obj);
    let shared_pointer_obj_2_clone = shared_pointer_obj_2.clone();

    let num_list = vec![1, 2, 3];
    let num_list_clone = num_list.clone();

    let handle = std::thread::spawn(move || {
        println!("{:#?}", num_list_clone);
    });

    handle.join().unwrap();

    println!("{:#?}", num_list);

    std::thread::sleep(duration);
}

pub fn test_val_thread_env() {
    let num_f = 21.3;
    let num_i = 33;
    let char = 'a';

    let handle = std::thread::spawn(move || {
        println!("{:?} : {:#?}", std::thread::current().id(), num_f);
        println!("{:?} : {:#?}", std::thread::current().id(), num_i);
        println!("{:?} : {:#?}", std::thread::current().id(), char);
    });

    handle.join().unwrap();

    println!("{:?} : {:#?}", std::thread::current().id(), num_f);
    println!("{:?} : {:#?}", std::thread::current().id(), num_i);
    println!("{:?} : {:#?}", std::thread::current().id(), char);
}

pub fn test_string_thread_env() {
    let string_obj = "test".to_string();
    let string_obj_clone = string_obj.clone();

    let handle = std::thread::spawn(move || {
        println!("{:?} : {:#?}", std::thread::current().id(), string_obj_clone);
    });

    handle.join().unwrap();

    println!("{:?} : {:#?}", std::thread::current().id(), string_obj);
}

pub fn test_copyable_struct(){
    let st=CopyableObj::new(1,2);
    let handle = std::thread::spawn(move || {
        println!("{:?} : {:#?}", std::thread::current().id(), st);
    });

    handle.join().unwrap();

    println!("{:?} : {:#?}", std::thread::current().id(), st);

}

pub fn test_uncopiable_struct(){
    let st=UncopiableObj::new("test".to_string());
    let st_clone=st.clone();

    let handle = std::thread::spawn(move || {
        println!("{:?} : {:#?}", std::thread::current().id(), st_clone);
    });

    handle.join().unwrap();

    println!("{:?} : {:#?}", std::thread::current().id(), st);
}

#[cfg(test)]
mod tests {
    use crate::thread_shared_obj::thread_test::*;

    #[test]
    fn test_val() {
        test_val_thread_env();
    }

    #[test]
    fn test_string() {
        test_string_thread_env();
    }

    #[test]
    fn t_test_copyable_struct(){
        test_copyable_struct();
    }

    #[test]
    fn t_test_uncopiable_struct(){
        test_uncopiable_struct();
    }
}
