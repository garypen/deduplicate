use std::hash::Hash;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::Mutex;

use lru::LruCache;

/// Internal Cache
///
/// Retrieved results are stored here.
#[derive(Clone)]
pub(crate) struct Cache<K: Hash + Eq + Send, V: Clone> {
    inner: Arc<Mutex<LruCache<K, V>>>,
}

impl<K, V> Cache<K, V>
where
    K: Hash + Eq + Send,
    V: Clone + Send,
{
    pub(crate) fn new(max_capacity: NonZeroUsize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(LruCache::new(max_capacity))),
        }
    }

    pub(crate) fn get(&self, key: &K) -> Option<V> {
        self.inner.lock().unwrap().get(key).cloned()
    }

    pub(crate) fn insert(&self, key: K, value: V) {
        self.inner.lock().unwrap().put(key, value);
    }

    pub(crate) fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }
}
