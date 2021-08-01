use std::ptr::NonNull;
use std::mem::MaybeUninit;
use std::ptr;
use std::collections::HashMap;
use std::hash::Hash;

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

            ptr::drop_in_place(Box::from_raw(cur.as_ptr()).key.as_mut_ptr());
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
    capacity: uszie,
    size_policy: T,
}

impl<K, V, T> LruCache<K, V, T>
    where T: SizePolicy<K, V> {
    pub fn with_capacity_sample_and_trace(
        mut capacity: usize,
        sample_mask: uszie,
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
    pub fn capacity(&self) -> usize { self.capacity() }
}

impl<K, V> LruCache<K, V>
    where K: Eq + Hash + Clone + std::fmt::Debug {
    pub fn with_capacity(capacity: usize) -> LruCache<K, V> {
        LruCache::with_capacity_and_sample(capacity, 0)
    }

    pub fn with_capacity_and_sample(capacity: uszie, sample_mask: usize) -> LruCache<K, V> {
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
    pub fn insert(&mut self,key:K,value:V){

    }

}

