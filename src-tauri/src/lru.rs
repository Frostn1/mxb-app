//! A bounded, least-recently-used map.
//!
//! The viewer caches parsed meshes and whole bike models so re-opening one is instant.
//! Both used to grow without a real ceiling — the mesh cache had none at all, and the bike
//! cache emptied itself wholesale once it hit six entries, throwing away the model you were
//! looking at along with the rest. This keeps the recent ones and evicts only the coldest,
//! handing the caller what it dropped so it can free anything hanging off it.

use std::collections::{HashMap, VecDeque};

pub struct Lru<V> {
    map: HashMap<String, V>,
    /// Coldest key first.
    order: VecDeque<String>,
    cap: usize,
}

impl<V> Lru<V> {
    pub fn new(cap: usize) -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
            cap: cap.max(1),
        }
    }

    /// Look up `key`, marking it as the most recently used.
    pub fn get(&mut self, key: &str) -> Option<&V> {
        if !self.map.contains_key(key) {
            return None;
        }
        self.touch(key);
        self.map.get(key)
    }

    /// Insert `value`, returning whatever was evicted to make room for it.
    pub fn insert(&mut self, key: String, value: V) -> Option<V> {
        if self.map.insert(key.clone(), value).is_some() {
            self.touch(&key);
            return None; // replaced in place, nothing left the cache
        }
        self.order.push_back(key);
        if self.map.len() > self.cap {
            if let Some(coldest) = self.order.pop_front() {
                return self.map.remove(&coldest);
            }
        }
        None
    }

    fn touch(&mut self, key: &str) {
        if let Some(i) = self.order.iter().position(|k| k == key) {
            let k = self.order.remove(i).expect("index came from the deque");
            self.order.push_back(k);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_the_coldest_entry() {
        let mut lru = Lru::new(2);
        assert!(lru.insert("a".into(), 1).is_none());
        assert!(lru.insert("b".into(), 2).is_none());
        assert_eq!(lru.insert("c".into(), 3), Some(1), "'a' was coldest");
        assert!(lru.get("a").is_none());
        assert_eq!(lru.get("b"), Some(&2));
        assert_eq!(lru.get("c"), Some(&3));
    }

    #[test]
    fn a_hit_keeps_an_entry_alive() {
        let mut lru = Lru::new(2);
        lru.insert("a".into(), 1);
        lru.insert("b".into(), 2);
        lru.get("a"); // 'a' is now the warm one
        assert_eq!(lru.insert("c".into(), 3), Some(2), "'b' went instead");
        assert_eq!(lru.get("a"), Some(&1));
    }

    #[test]
    fn replacing_a_key_evicts_nothing() {
        let mut lru = Lru::new(2);
        lru.insert("a".into(), 1);
        lru.insert("b".into(), 2);
        assert_eq!(lru.insert("a".into(), 9), None);
        assert_eq!(lru.get("a"), Some(&9));
        assert_eq!(lru.get("b"), Some(&2), "both still resident");
    }
}
