//! Provides a safe, asynchronous (tokio based), caching request deduplicator.
//!
//! If you have a slow or expensive data retrieving operation data, [`Deduplicate`] will help avoid
//! work duplication. Furthermore, if your retrieval operation is "flaky", failures will be handled
//! cleanly and [`Deduplicate`] will continue to function.
//!
//! An example will probably make usage clear.
//!
//!
//! Let's imagine we have a mechanism for retrieving data that is arbitrarily slow. Our data is
//! keyed by `usize` and consists of a `String`. In this case we are simply going to sleep for a
//! while and then return a value which consists of the supplied key and the duration for which we
//! slept. We represent this as an async fn or a closure which takes a single argument, being the
//! key of the data we are working with, and returns an optional value.
//!
//! We can create a [`Deduplicate`] instance to manage delegated access our slow function and
//! ensure that concurrent calls to get the same key are not duplicated.
//!
//! Example 1
//! ```
//! use std::future::Future;
//! use std::pin::Pin;
//! use deduplicate::Deduplicate;
//! use deduplicate::DeduplicateError;
//!
//! use rand::Rng;
//!
//!
//! fn get(key: usize) -> Pin<Box<dyn Future<Output = Option<String>> + Send + 'static>> {
//!     let fut = async move {
//!         let num = rand::thread_rng().gen_range(1000..2000);
//!         tokio::time::sleep(tokio::time::Duration::from_millis(num)).await;
//!
//!         Some(format!("key: {}, duration: {}", key, num))
//!     };
//!     Box::pin(fut)
//! }
//!
//!
//! let closure = |key: usize| {
//!     let fut = async move {
//!         let num = rand::thread_rng().gen_range(1000..2000);
//!         tokio::time::sleep(tokio::time::Duration::from_millis(num)).await;
//!
//!         Some(format!("key: {}, duration: {}", key, num))
//!     };
//!     Box::pin(fut) as Pin<Box<dyn Future<Output = Option<String>> + Send + 'static>>
//! };
//!
//! let deduplicate_with_fn = Deduplicate::new(get);
//! let deduplicate_with_closure = Deduplicate::new(closure);
//! ```
//!
//! Now we can invoke get concurrently on our deduplicator and be sure that the expensive retrieve
//! function (or closure) is only executed once for all concurrent requests. Furthermore, if we
//! don't disable the cache, then the results are cached for future requests and this can further
//! speed up access times.
//!
mod cache;
mod deduplicate;

pub use crate::deduplicate::Deduplicate;
pub use crate::deduplicate::DeduplicateError;
