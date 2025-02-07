// A simple bijective hashmap.
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Deref;
use std::rc::Rc;

#[derive(Debug, Clone, Default)]
pub struct BiHashMap<K, V> {
    map: HashMap<Rc<K>, Rc<V>>,
    reverse_map: HashMap<Rc<V>, Rc<K>>,
}

impl<K, V> BiHashMap<K, V>
where
    K: Eq + std::hash::Hash + Copy,
    V: Eq + std::hash::Hash + Copy,
{
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            reverse_map: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: K, value: V) {
        let key = Rc::new(key);
        let value = Rc::new(value);

        // Remove the old key if it exists
        if let Some(old_value) = self.map.remove(&key) {
            self.reverse_map.remove(&old_value);
        }
        // Remove the old value if it exists
        if let Some(old_key) = self.reverse_map.remove(&value) {
            self.map.remove(&old_key);
        }

        self.map.insert(key.clone(), value.clone());
        self.reverse_map.insert(value, key);
    }

    #[inline]
    pub fn get(&self, k: &K) -> Option<&V> {
        self.map.get(k).map(Deref::deref)
    }

    pub fn get_reverse(&self, value: &V) -> Option<&K> {
        self.reverse_map.get(value).map(Deref::deref)
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        if let Some(value) = self.map.remove(key) {
            self.reverse_map.remove(&value);
            Some(*value)
        } else {
            None
        }
    }

    pub fn remove_reverse(&mut self, value: &V) -> Option<K> {
        if let Some(key) = self.reverse_map.remove(value) {
            self.map.remove(&key);
            Some(*key)
        } else {
            None
        }
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }

    pub fn contains_value(&self, value: &V) -> bool {
        self.reverse_map.contains_key(value)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.reverse_map.clear();
    }
}
