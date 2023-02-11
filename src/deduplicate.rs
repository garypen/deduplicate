use crate::cache::Cache;
use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Weak;

use thiserror::Error;
use tokio::sync::broadcast;
use tokio::sync::Mutex;

/// Boxed Future yielding an optional value.
pub type DeduplicateFuture<V> = Pin<Box<dyn Future<Output = Option<V>> + Send>>;

type WaitMap<K, V> = Arc<Mutex<HashMap<K, Weak<broadcast::Sender<Option<V>>>>>>;

const DEFAULT_CACHE_CAPACITY: usize = 512;

/// Deduplication errors.
#[derive(Debug, Error)]
pub enum DeduplicateError {
    /// The delegated get failed.
    #[error("Delegated get failed")]
    Failed,
    /// There is no cache in this instance.
    #[error("Cache not enabled")]
    NoCache,
}

/// Query de-duplication with optional cache.
///
/// When trying to avoid multiple slow or expensive gets, use this.
#[derive(Clone)]
pub struct Deduplicate<G, K, V>
where
    G: Fn(K) -> DeduplicateFuture<V>,
    K: Clone + Send + Eq + Hash,
    V: Clone + Send,
{
    delegate: G,
    storage: Option<Cache<K, V>>,
    wait_map: WaitMap<K, V>,
    request_deduplicated_counter: Arc<AtomicU64>,
    request_total_counter: Arc<AtomicU64>,
}

impl<G, K, V> Deduplicate<G, K, V>
where
    G: Fn(K) -> DeduplicateFuture<V>,
    K: Clone + Send + Eq + Hash + 'static,
    V: Clone + Send + 'static,
{
    /// Create a new deduplicator for the provided delegate with default cache capacity: 512.
    pub fn new(delegate: G) -> Self {
        Self::with_capacity(delegate, DEFAULT_CACHE_CAPACITY)
    }

    /// Create a new deduplicator for the provided delegate with specified cache capacity.
    /// Note: If capacity is 0, then caching is disabled.
    pub fn with_capacity(delegate: G, capacity: usize) -> Self {
        let storage = if capacity > 0 {
            let val = unsafe { NonZeroUsize::new_unchecked(capacity) };
            Some(Cache::new(val))
        } else {
            None
        };
        Self {
            delegate,
            wait_map: Arc::new(Mutex::new(HashMap::new())),
            storage,
            request_deduplicated_counter: Arc::new(AtomicU64::new(0)),
            request_total_counter: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Clear the internal cache. This will also reset the request counters.
    pub fn clear(&self) {
        if let Some(storage) = &self.storage {
            storage.clear();
        }
        self.request_deduplicated_counter.store(0, Ordering::SeqCst);
        self.request_total_counter.store(0, Ordering::SeqCst);
    }

    /// Return the number of cache entries in use. Will return 0 if no cache is configured.
    pub fn count(&self) -> usize {
        match &self.storage {
            Some(s) => s.count(),
            None => 0,
        }
    }

    /// Return the deduplicated request count.
    pub fn request_deduplicated_count(&self) -> u64 {
        self.request_deduplicated_counter.load(Ordering::SeqCst)
    }

    /// Return the total request count.
    pub fn request_count(&self) -> u64 {
        self.request_total_counter.load(Ordering::SeqCst)
    }

    /// Use the delegate to get a value.
    ///
    /// Many concurrent accessors can attempt to get the same key, but the underlying get will only
    /// be called once. If the delegate panics or is cancelled, any concurrent accessors will get the
    /// error: [`DeduplicateError::Failed`].
    // Disable clippy false positive. We are explicitly dropping our lock, so clippy is wrong.
    #[allow(clippy::await_holding_lock)]
    pub async fn get(&self, key: K) -> Result<Option<V>, DeduplicateError> {
        self.request_total_counter.fetch_add(1, Ordering::SeqCst);
        let mut locked_wait_map = self.wait_map.lock().await;
        match locked_wait_map.get(&key) {
            Some(weak) => {
                self.request_deduplicated_counter
                    .fetch_add(1, Ordering::SeqCst);
                if let Some(strong) = weak.upgrade() {
                    let mut receiver = strong.subscribe();
                    // Very important to drop this...
                    drop(strong);
                    drop(locked_wait_map);
                    // Note. It may seem that there is a race condition here, but we have managed
                    // to upgrade our weak reference and we have subscribed to our broadcast before
                    // we dropped our lock. Because we only send messages whilst holding the lock,
                    // we know that it must be safe to recv() here. We still handle errors, but
                    // don't expect to receive any.
                    receiver.recv().await.map_err(|_| DeduplicateError::Failed)
                } else {
                    // In the normal run of things, we won't reach this code. However, if a
                    // delegate panics and fails to complete or a task is cancelled at an .await
                    // then we may find ourselves here. If so, we may have lost our sender
                    // and there may still be an entry in the wait_map which our receiver has not
                    // yet removed. In which case, we don't have a value and we'll never get a
                    // value, so let's just remove the wait_map entry and return Failed.
                    let _ = locked_wait_map.remove(&key);
                    Err(DeduplicateError::Failed)
                }
            }
            None => {
                let (sender, mut receiver) = broadcast::channel(1);
                let sender = Arc::new(sender);
                locked_wait_map.insert(key.clone(), Arc::downgrade(&sender));

                drop(locked_wait_map);
                if let Some(storage) = &self.storage {
                    if let Some(value) = storage.get(&key) {
                        let mut locked_wait_map = self.wait_map.lock().await;
                        let _ = locked_wait_map.remove(&key);
                        let _ = sender.send(Some(value.clone()));

                        self.request_deduplicated_counter
                            .fetch_add(1, Ordering::SeqCst);
                        return Ok(Some(value));
                    }
                }
                let fut = (self.delegate)(key.clone());
                let k = key.clone();
                let wait_map = self.wait_map.clone();
                tokio::spawn(async move {
                    let value = fut.await;
                    // Clean up the wait map before we send the value
                    let mut locked_wait_map = wait_map.lock().await;
                    let _ = locked_wait_map.remove(&k);
                    let _ = sender.send(value);
                });
                // We only want one receiver to clean up the wait map, so this is the right place
                // to do it.
                let result = receiver.recv().await.map_err(|_| DeduplicateError::Failed);
                let mut locked_wait_map = self.wait_map.lock().await;
                let _ = locked_wait_map.remove(&key);
                let res = result?;
                if let Some(storage) = &self.storage {
                    if let Some(v) = &res {
                        storage.insert(key, v.clone());
                    }
                }
                Ok(res)
            }
        }
    }

    /// Insert an entry directly into the cache. If there is no cache , this
    /// will fail with [`DeduplicateError::NoCache`].
    pub fn insert(&self, key: K, value: V) -> Result<(), DeduplicateError> {
        if let Some(storage) = &self.storage {
            storage.insert(key, value);
            Ok(())
        } else {
            Err(DeduplicateError::NoCache)
        }
    }

    /// Update the delegate to use for future gets. This will also clear the internal cache and
    /// reset the request counters.
    pub fn set_delegate(&mut self, delegate: G) {
        self.clear();
        self.delegate = delegate;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;
    use std::time::Instant;

    fn get(_key: usize) -> DeduplicateFuture<String> {
        let fut = async {
            let num = rand::thread_rng().gen_range(1000..2000);
            tokio::time::sleep(tokio::time::Duration::from_millis(num)).await;

            if num % 2 == 0 {
                panic!("BAD NUMBER");
            }
            Some("test".to_string())
        };
        Box::pin(fut)
    }

    async fn test_harness<G>(deduplicate: Deduplicate<G, usize, String>)
    where
        G: Fn(usize) -> DeduplicateFuture<String>,
    {
        // Let's create our normal getter and use our deduplicating delegate.
        // (The same functionality as `get`, but without panicking.)
        let no_panic_get = |_x: usize| async {
            let num = rand::thread_rng().gen_range(1000..2000);
            tokio::time::sleep(tokio::time::Duration::from_millis(num)).await;

            Some("test".to_string())
        };
        let deduplicate = Arc::new(deduplicate);

        // We are going to perform the work 5 times to be sure our de-duplicator is working
        for i in 1..6 {
            let mut dedup_hdls = vec![];
            let mut slower_hdls = vec![];
            let start = Instant::now();
            // Create our lists of dedup and non-dedup futures
            for _i in 0..100 {
                let my_deduplicate = deduplicate.clone();
                dedup_hdls.push(async move {
                    let is_ok = my_deduplicate.get(5).await.is_ok();
                    (Instant::now(), is_ok)
                });
                slower_hdls.push(async move {
                    let is_ok = (no_panic_get)(5).await.is_some();
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
            println!("iteration: {i}");
            println!("dedup_range: {dedup_range:?}");
            println!("slower_range: {slower_range:?}");
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
            println!("dedup passed: {dedup_passed:?}");
            println!("slower passed: {slower_passed:?}");
            println!("elapsed: {:?}\n", Instant::now() - start);
        }
    }

    // Test that deduplication works with a default cache.
    #[tokio::test]
    async fn it_deduplicates_correctly_with_cache() {
        let no_panic_get = |_x: usize| -> DeduplicateFuture<String> {
            let fut = async {
                let num = rand::thread_rng().gen_range(1000..2000);
                tokio::time::sleep(tokio::time::Duration::from_millis(num)).await;

                Some("test".to_string())
            };
            Box::pin(fut)
        };
        test_harness(Deduplicate::new(no_panic_get)).await
    }

    // Test that deduplication works with no cache.
    #[tokio::test]
    async fn it_deduplicates_correctly_without_cache() {
        test_harness(Deduplicate::with_capacity(get, 0)).await
    }
}
