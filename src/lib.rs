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
//! keyed by `usize` and consists of a `String`. To emulate this, we are simply going to sleep for a
//! while and then return a value which consists of the supplied key and the duration for which we
//! slept. We represent this as a function or a closure which takes a single argument, being the
//! key of the data we are working with, and returns a [`DeduplicateFuture`] which yields an optional value.
//!
//! We can create a [`Deduplicate`] instance to manage delegated access via our slow function and
//! ensure that concurrent calls to get the same key are not duplicated.
//!
//! Example 1
//! ```
//! use deduplicate::DeduplicateFuture;
//! use deduplicate::Deduplicate;
//! use deduplicate::DeduplicateError;
//!
//! use rand::Rng;
//!
//!
//! // This is our slow accessor function. Note that we must take a single
//! // key argument and return a [`DeduplicateFuture`] with our value.
//! // All of our specific logic is enclosed within an async block. We
//! // are using move to move the key into the block.  Finally, we pin
//! // the block and return it.
//! fn get(key: usize) -> DeduplicateFuture<String> {
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
//! // All the comments from the get function apply here. In this case
//! // we are choosing to provide a closure rather than a function.
//! let closure = |key: usize| -> DeduplicateFuture<String> {
//!     let fut = async move {
//!         let num = rand::thread_rng().gen_range(1000..2000);
//!         tokio::time::sleep(tokio::time::Duration::from_millis(num)).await;
//!
//!         Some(format!("key: {}, duration: {}", key, num))
//!     };
//!     Box::pin(fut)
//! };
//!
//! // We create two deduplicate instances, one from our function and one
//! // from our closure for purposes of illustration. We'd only create one
//! // in a real application.
//! let deduplicate_with_fn = Deduplicate::new(get);
//! let deduplicate_with_closure = Deduplicate::new(closure);
//! // Our get is async, so use tokio_test::block_on to execute it.
//! let value = tokio_test::block_on(deduplicate_with_fn.get(42));
//! println!("value: {:?}", value);
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
pub use crate::deduplicate::DeduplicateFuture;
