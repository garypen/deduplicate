use parking_lot::Mutex;
use std::hash::Hash;
use std::num::NonZeroUsize;
use std::sync::Arc;

use lru::LruCache;
// use lru_cache::LruCache;

/// Internal Cache
///
/// Retrieved results are stored here.
#[derive(Clone)]
pub(crate) struct Cache<K: Clone + Hash + Eq + Send, V: Clone> {
    inner: Arc<Mutex<LruCache<K, V>>>,
}

impl<K, V> Cache<K, V>
where
    K: Clone + Hash + Eq + Send,
    V: Clone + Send,
{
    pub(crate) fn new(max_capacity: NonZeroUsize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(LruCache::new(max_capacity))),
        }
    }

    pub(crate) fn get(&self, key: &K) -> Option<V> {
        self.inner.lock().get(key).cloned()
    }

    pub(crate) fn insert(&self, key: K, value: V) {
        self.inner.lock().put(key, value);
    }

    pub(crate) fn clear(&self) {
        self.inner.lock().clear();
    }

    pub(crate) fn count(&self) -> usize {
        self.inner.lock().len()
    }

    pub(crate) fn entries(&self) -> Vec<(K, V)> {
        self.inner
            .lock()
            .iter()
            .map(|(k, v)| ((*k).clone(), v.clone()))
            .collect()
    }
}
