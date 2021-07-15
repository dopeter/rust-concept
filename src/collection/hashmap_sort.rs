use std::collections::HashMap;

pub fn sort(){

    let mut m=HashMap::new();

    m.insert(1,"aa");
    m.insert(3,"bb");
    m.insert(9,"cc");

    m.iter().for_each(|em|println!("{},{}", em.0,em.1));

}



#[cfg(test)]
mod tests {
    use crate::collection::hashmap_sort::sort;

    #[test]
    fn test_val() {
        sort()
    }


}