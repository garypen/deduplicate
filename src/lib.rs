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
//! slept. We represent this by implementing the [`Retriever`] trait.
//!
//! Once we have a retriever, we can create a [`Deduplicate`] instance to manage delegate access to
//! data efficiently.
//!
//! Example 1
//! ```
//! use deduplicate::Deduplicate;
//! use deduplicate::DeduplicateError;
//! use deduplicate::Retriever;
//!
//! use rand::Rng;
//!
//! struct MyRetriever;
//!
//! #[async_trait::async_trait]
//! impl Retriever for MyRetriever {
//!     type Key = usize;
//!     type Value = String;
//!
//!     async fn get(&self, key: &Self::Key) -> Option<Self::Value> {
//!         let num = rand::thread_rng().gen_range(4000..5000);
//!         let duration = tokio::time::Duration::from_millis(num);
//!         tokio::time::sleep(duration).await;
//!         Some(format!("key: {}, duration: {}", key, num))
//!     }
//! }
//! ```
//!
//!
mod cache;
mod deduplicate;

pub use crate::deduplicate::Deduplicate;
pub use crate::deduplicate::DeduplicateError;
pub use crate::deduplicate::Retriever;
