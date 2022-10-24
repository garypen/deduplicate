use crate::cache::Cache;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::sync::Weak;

use thiserror::Error;
use tokio::sync::broadcast;
use tokio::sync::Mutex;

type WaitMap<K, V> = Arc<Mutex<HashMap<K, Weak<broadcast::Sender<Option<V>>>>>>;

const DEFAULT_CACHE_CAPACITY: usize = 512;

/// Deduplication errors
#[derive(Debug, Error)]
pub enum DeduplicateError {
    #[error("Delegated retrieve failed")]
    Failed,
    #[error("Cache not enabled")]
    NoCache,
    #[error("Delegated retrieve not found")]
    NotFound,
}

/// Delegated retrieval trait.
///
/// This is the slow or expensive get that we are de-duplicating.
#[async_trait::async_trait]
pub trait Retriever: Send + Sync {
    type Key;
    type Value;

    async fn get(&self, key: &Self::Key) -> Option<Self::Value>;
}

/// Query de-duplication with optional cache.
///
/// When trying to avoid multiple slow or expensive retrievals, use this.
#[derive(Clone)]
pub struct Deduplicate<K: Clone + Send + Eq + Hash, V: Clone + Send> {
    retriever: Arc<dyn Retriever<Key = K, Value = V>>,
    storage: Option<Cache<K, V>>,
    wait_map: WaitMap<K, V>,
}

impl<K, V> Deduplicate<K, V>
where
    K: Clone + Send + Eq + Hash + 'static,
    V: Clone + Send + 'static,
{
    /// Create a new deduplicator for the provided retriever with default cache capacity: 512.
    pub async fn new(retriever: Arc<dyn Retriever<Key = K, Value = V>>) -> Self {
        Self::with_capacity(retriever, DEFAULT_CACHE_CAPACITY).await
    }

    /// Create a new deduplicator for the provided retriever with specified cache capacity.
    /// Note: If capacity is 0, then caching is disabled.
    pub async fn with_capacity(
        retriever: Arc<dyn Retriever<Key = K, Value = V>>,
        capacity: usize,
    ) -> Self {
        let storage = if capacity > 0 {
            Some(Cache::new(capacity))
        } else {
            None
        };
        Self {
            retriever,
            wait_map: Arc::new(Mutex::new(HashMap::new())),
            storage,
        }
    }

    /// Update the retriever to use for future gets. This will also clear the internal cache.
    pub fn set_retriever(&mut self, retriever: Arc<dyn Retriever<Key = K, Value = V>>) {
        self.clear();
        self.retriever = retriever;
    }

    /// Clear the internal cache.
    pub fn clear(&mut self) {
        if let Some(storage) = &self.storage {
            storage.clear();
        }
    }

    /// Use the retriever to get a value. If the key cannot be retrieved, then a
    /// [`DeduplicateError::NotFound`] error is returned. Many concurrent accessors can
    /// attempt to get the same key, but the retriever will only be used once. It is
    /// possible that the retriever will panic. In which case any concurrent accessors
    /// will get the error: [`DeduplicateError::Failed`]
    pub async fn get(&self, key: &K) -> Result<V, DeduplicateError> {
        let mut locked_wait_map = self.wait_map.lock().await;
        match locked_wait_map.get(key) {
            Some(weak) => {
                if let Some(strong) = weak.upgrade() {
                    let mut receiver = strong.subscribe();
                    // Very important to drop this...
                    drop(strong);
                    drop(locked_wait_map);
                    receiver
                        .recv()
                        .await
                        .map_err(|_| DeduplicateError::Failed)?
                        .ok_or(DeduplicateError::NotFound)
                } else {
                    // Because we clean up the wait map from the receiver, it will be a logic error
                    // if we end up here. Panic is the best choice so the bug can be fixed.
                    panic!("a stray waiter is still in the wait map for one of our keys");
                }
            }
            None => {
                let (sender, mut receiver) = broadcast::channel(1);
                let sender = Arc::new(sender);
                locked_wait_map.insert(key.clone(), Arc::downgrade(&sender));

                drop(locked_wait_map);
                if let Some(storage) = &self.storage {
                    if let Some(value) = storage.get(key) {
                        let mut locked_wait_map = self.wait_map.lock().await;
                        let _ = locked_wait_map.remove(key);
                        let _ = sender.send(Some(value.clone()));

                        return Ok(value);
                    }
                }
                let retriever = self.retriever.clone();
                let k = key.clone();
                tokio::spawn(async move {
                    let fut = retriever.get(&k);
                    let value = fut.await;
                    let _ = sender.send(value);
                });
                // We only want one receiver to clean up the wait map, so this is the right place
                // to do it.
                let result = receiver.recv().await.map_err(|_| DeduplicateError::Failed);
                let mut locked_wait_map = self.wait_map.lock().await;
                let _ = locked_wait_map.remove(key);
                let res = result?.ok_or(DeduplicateError::NotFound);
                if let Some(storage) = &self.storage {
                    if let Ok(v) = &res {
                        storage.insert(key.clone(), v.clone());
                    }
                }
                res
            }
        }
    }

    /// Insert an entry directly into the cache.
    pub fn insert(&mut self, key: K, value: V) -> Result<(), DeduplicateError> {
        if let Some(storage) = &self.storage {
            storage.insert(key, value);
            Ok(())
        } else {
            Err(DeduplicateError::NoCache)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;
    use std::time::Instant;

    struct MockRetriever(bool);

    #[async_trait::async_trait]
    impl Retriever for MockRetriever {
        type Key = usize;
        type Value = String;

        async fn get(&self, _key: &Self::Key) -> Option<Self::Value> {
            let num = rand::thread_rng().gen_range(1000..2000);
            tokio::time::sleep(tokio::time::Duration::from_millis(num)).await;

            if self.0 && num % 2 == 0 {
                panic!("BAD NUMBER");
            }
            Some("test".to_string())
        }
    }

    impl MockRetriever {
        fn new(may_panic: bool) -> Self {
            MockRetriever(may_panic)
        }
    }

    async fn test_harness(deduplicate: Deduplicate<usize, String>) {
        // Let's create our normal retriever and our deduplicating retriever
        let slower = Arc::new(MockRetriever::new(false));
        let deduplicate = Arc::new(deduplicate);

        // We are going to perform the work 5 times to be sure our de-duplicator is working
        for i in 1..6 {
            let mut dedup_hdls = vec![];
            let mut slower_hdls = vec![];
            let start = Instant::now();
            // Create our lists of dedup and non-dedup futures
            for _i in 0..100 {
                let my_deduplicate = deduplicate.clone();
                let my_slower = slower.clone();
                dedup_hdls.push(async move {
                    let is_ok = my_deduplicate.get(&5).await.is_ok();
                    (Instant::now(), is_ok)
                });
                slower_hdls.push(async move {
                    let is_ok = my_slower.get(&5).await.is_some();
                    (Instant::now(), is_ok)
                });
            }
            // Execute our futures and collect the results
            let mut dedup_result: Vec<(Instant, bool)> = futures::future::join_all(dedup_hdls)
                .await
                .into_iter()
                .collect();
            dedup_result.sort();
            let mut slower_result: Vec<(Instant, bool)> = futures::future::join_all(slower_hdls)
                .await
                .into_iter()
                .collect();
            slower_result.sort();
            // Calculate the range of timings for each set of futures
            let dedup_range = dedup_result.last().unwrap().0 - dedup_result.first().unwrap().0;
            let slower_range = slower_result.last().unwrap().0 - slower_result.first().unwrap().0;
            println!("iteration: {}", i);
            println!("dedup_range: {:?}", dedup_range);
            println!("slower_range: {:?}", slower_range);
            // The dedup range should be a few ms. The slower range will tend towards 1 second.
            // It's very unlikely that this assertion will be false, but I should note that it is
            // possible... In which case, ignore it and re-run the test.
            assert!(dedup_range <= slower_range);
            // The number of passing tests will be <= slower for dedup because of the possibility
            // of a panic
            let dedup_passed = dedup_result
                .iter()
                .fold(0, |acc, x| if x.1 { acc + 1 } else { acc });
            let slower_passed = slower_result
                .iter()
                .fold(0, |acc, x| if x.1 { acc + 1 } else { acc });
            // for dedup, panic == 0 passes, no panic == 100 passes
            assert!(dedup_passed == 0 || dedup_passed == 100);
            assert_eq!(slower_passed, 100);
            assert!(dedup_passed <= slower_passed);
            println!("dedup passed: {:?}", dedup_passed);
            println!("slower passed: {:?}", slower_passed);
            println!("elapsed: {:?}\n", Instant::now() - start);
        }
    }

    #[tokio::test]
    async fn it_deduplicates_correctly_with_cache() {
        test_harness(Deduplicate::new(Arc::new(MockRetriever::new(true))).await).await
    }

    #[tokio::test]
    async fn it_deduplicates_correctly_without_cache() {
        test_harness(Deduplicate::with_capacity(Arc::new(MockRetriever::new(true)), 0).await).await
    }
}
