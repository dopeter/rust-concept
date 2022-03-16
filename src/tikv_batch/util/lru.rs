use std::ptr::NonNull;
use std::mem::MaybeUninit;
use std::ptr;
use std::collections::HashMap;
use std::hash::Hash;
use std::collections::hash_map::Entry as HashMapEntry;

struct Record<K> {
    prev: NonNull<Record<K>>,
    next: NonNull<Record<K>>,
    key: MaybeUninit<K>,
}

struct ValueEntry<K, V> {
    value: V,
    record: NonNull<Record<K>>,
}

//region Trace
struct Trace<K> {
    head: Box<Record<K>>,
    tail: Box<Record<K>>,
    tick: usize,
    sample_mask: usize,
}

impl<K> Trace<K> {
    fn new(sample_mask: usize) -> Trace<K> {
        unsafe {
            let mut head = Box::new(Record {
                prev: NonNull::new_unchecked(1usize as _),
                next: NonNull::new_unchecked(1usize as _),
                key: MaybeUninit::uninit(),
            });

            let mut tail = Box::new(Record {
                prev: NonNull::new_unchecked(1usize as _),
                next: NonNull::new_unchecked(1usize as _),
                key: MaybeUninit::uninit(),
            });

            suture(&mut head, &mut tail);

            Trace {
                head,
                tail,
                sample_mask,
                tick: 0,
            }
        }
    }

    #[inline]
    fn maybe_promote(&mut self, record: NonNull<Record<K>>) {
        self.tick += 1;
        if self.tick & self.sample_mask == 0 {
            self.promote(record);
        }
    }

    fn promote(&mut self, mut record: NonNull<Record<K>>) {
        unsafe {
            cut_out(record.as_mut());
            suture(record.as_mut(), &mut self.head.next.as_mut());
            suture(&mut self.head, record.as_mut());
        }
    }

    fn delete(&mut self, mut record: NonNull<Record<K>>) {
        unsafe {
            cut_out(record.as_mut());

            ptr::drop_in_place(Box::from_raw(record.as_ptr()).key.as_mut_ptr());
        }
    }

    fn create(&mut self, key: K) -> NonNull<Record<K>> {
        let record = Box::leak(Box::new(Record {
            prev: unsafe { NonNull::new_unchecked(&mut *self.head) },
            next: self.head.next,
            key: MaybeUninit::new(key),
        })).into();

        unsafe {
            self.head.next.as_mut().prev = record;
            self.head.next = record;
        }

        record
    }

    fn reuse_tail(&mut self, key: K) -> (K, NonNull<Record<K>>) {
        unsafe {
            let mut record = self.tail.prev;
            cut_out(record.as_mut());
            suture(record.as_mut(), self.head.next.as_mut());
            suture(&mut self.head, record.as_mut());

            let old_key = record.as_mut().key.as_ptr().read();
            record.as_mut().key = MaybeUninit::new(key);
            (old_key, record)
        }
    }

    fn clear(&mut self) {
        let mut cur = self.head.next;
        unsafe {
            while cur.as_ptr() != &mut *self.tail {
                let tmp = cur.as_mut().next;
                ptr::drop_in_place(Box::from_raw(cur.as_ptr()).key.as_mut_ptr());
                cur = tmp;
            }
            suture(&mut self.head, &mut self.tail);
        }
    }

    fn remove_tail(&mut self) -> K {
        unsafe {
            let mut record = self.tail.prev;
            cut_out(record.as_mut());

            let r = Box::from_raw(record.as_ptr());
            r.key.as_ptr().read()
        }
    }
}

//endregion


#[inline]
unsafe fn suture<K>(leading: &mut Record<K>, following: &mut Record<K>) {
    leading.next = NonNull::new_unchecked(following);
    following.prev = NonNull::new_unchecked(leading);
}

#[inline]
unsafe fn cut_out<K>(record: &mut Record<K>) {
    suture(record.prev.as_mut(), record.next.as_mut())
}

pub trait SizePolicy<K, V> {
    fn current(&self) -> usize;
    fn on_insert(&mut self, key: &K, value: &V);
    fn on_remove(&mut self, key: &K, value: &V);
    fn on_reset(&mut self, val: usize);
}

pub struct CountTracker(usize);

impl<K, V> SizePolicy<K, V> for CountTracker {
    fn current(&self) -> usize {
        self.0
    }

    fn on_insert(&mut self, key: &K, value: &V) {
        self.0 += 1;
    }

    fn on_remove(&mut self, key: &K, value: &V) {
        self.0 -= 1;
    }

    fn on_reset(&mut self, val: usize) {
        self.0 = val;
    }
}

impl Default for CountTracker {
    fn default() -> Self {
        Self(0)
    }
}


pub struct LruCache<K, V, T = CountTracker>
    where T: SizePolicy<K, V> {
    map: HashMap<K, ValueEntry<K, V>>,
    trace: Trace<K>,
    capacity: usize,
    size_policy: T,
}

impl<K, V, T> LruCache<K, V, T>
    where T: SizePolicy<K, V> {
    pub fn with_capacity_sample_and_trace(
        mut capacity: usize,
        sample_mask: usize,
        size_policy: T,
    ) -> LruCache<K, V, T> {
        if capacity == 0 {
            capacity = 1;
        }

        LruCache {
            map: HashMap::default(),
            trace: Trace::new(sample_mask),
            capacity,
            size_policy,
        }
    }

    #[inline]
    pub fn size(&self) -> usize { self.size_policy.current() }

    #[inline]
    pub fn clear(&mut self) {
        self.map.clear();
        self.trace.clear();
        self.size_policy.on_reset(0);
    }

    #[inline]
    pub fn capacity(&self) -> usize { self.capacity }
}

impl<K, V> LruCache<K, V>
    where K: Eq + Hash + Clone + std::fmt::Debug {
    pub fn with_capacity(capacity: usize) -> LruCache<K, V> {
        LruCache::with_capacity_and_sample(capacity, 0)
    }

    pub fn with_capacity_and_sample(capacity: usize, sample_mask: usize) -> LruCache<K, V> {
        LruCache::with_capacity_sample_and_trace(capacity, sample_mask, CountTracker::default())
    }

    #[inline]
    pub fn resize(&mut self, mut new_cap: usize) {
        if new_cap == 0 {
            new_cap = 1;
        }

        if new_cap < self.capacity() && self.map.len() > new_cap {
            for _ in new_cap..self.map.len() {
                let key = self.trace.remove_tail();
                let entry = self.map.remove(&key).unwrap();
                self.size_policy.on_remove(&key, &entry.value);
            }
            self.map.shrink_to_fit();
        }

        self.capacity = new_cap;
    }
}

impl<K, V, T> LruCache<K, V, T>
    where K: Eq + Hash + Clone + std::fmt::Debug,
          T: SizePolicy<K, V> {
    #[inline]
    pub fn insert(&mut self, key: K, value: V) {
        let mut old_key = None;
        let current_size = SizePolicy::<K, V>::current(&self.size_policy);
        match self.map.entry(key) {
            HashMapEntry::Occupied(mut e) => {
                self.size_policy.on_remove(e.key(), &e.get().value);
                self.size_policy.on_insert(e.key(), &value);
                let mut entry = e.get_mut();
                self.trace.promote(entry.record);
                entry.value = value;
            }
            HashMapEntry::Vacant(v) => {
                let record = if self.capacity <= current_size {
                    let res = self.trace.reuse_tail(v.key().clone());
                    old_key = Some(res.0);
                    res.1
                } else {
                    self.trace.create(v.key().clone())
                };

                self.size_policy.on_insert(v.key(), &value);
                v.insert(ValueEntry { value, record });
            }
        }

        if let Some(o) = old_key {
            let entry = self.map.remove(&o).unwrap();
            self.size_policy.on_remove(&o, &entry.value);
        }
    }

    #[inline]
    pub fn remove(&mut self, key: &K) -> Option<V> {
        if let Some(v) = self.map.remove(key) {
            self.trace.delete(v.record);
            self.size_policy.on_remove(key, &v.value);
            return Some(v.value);
        }
        None
    }

    #[inline]
    pub fn get(&mut self, key: &K) -> Option<&V> {
        match self.map.get_mut(key) {
            Some(v) => {
                self.trace.maybe_promote(v.record);
                Some(&v.value)
            }
            None => None
        }
    }

    #[inline]
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        match self.map.get_mut(key) {
            Some(v) => {
                self.trace.maybe_promote(v.record);
                return Some(&mut v.value);
            }
            None => None
        }
    }

    pub fn iter(&self) -> Iter<K, V> {
        Iter {
            base: self.map.iter()
        }
    }

    pub fn len(&self) -> usize { self.map.len() }

    pub fn is_empty(&self) -> bool { self.map.is_empty() }
}

unsafe impl<K, V, T> Send for LruCache<K, V, T>
    where K: Send, V: Send, T: Send + SizePolicy<K, V>
{}

impl<K,V,T> Drop for LruCache<K,V,T>
where T:SizePolicy<K,V>{
    fn drop(&mut self) {
        self.clear();
    }
}


pub struct Iter<'a, K: 'a, V: 'a> {
    base: std::collections::hash_map::Iter<'a, K, ValueEntry<K, V>>,
}

impl<'a,K,V> Iterator for Iter<'a,K,V>{
    type Item = (&'a K,&'a V);

    fn next(&mut self) -> Option<Self::Item> {
        self.base.next().map(|(k,v)|(k,&v.value))
    }
}


mod tests{

    use super::*;

    #[test]
    fn test_insert(){
        let mut map=LruCache::with_capacity(10);
        for i in 0..10{
            map.insert(i,i);
        }

    }

}




