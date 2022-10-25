# deduplicate
asynchronous deduplicator with optional LRU caching

If you have "slow", "expensive" or "flaky" tasks for which you'd like to provide de-duplication and, optionally, result caching, then this may be the crate you are looking for.

The crate provides a trait `Retriever` which should be implemented for your task. Once implemented, you can provide an instance of a `Retriever` to a `Deduplicate` which will then handle task de-duplication for you.

[![Crates.io](https://img.shields.io/crates/v/deduplicate.svg)](https://crates.io/crates/deduplicate)

[API Docs](https://docs.rs/deduplicate/latest/deduplicate)

## Installation

```toml
[dependencies]
deduplicate = "0.1"
```

## Acknowledgements

This crate build upon the hard work and inspiration of several folks, some of whom I have worked with directly and some from whom I have taken indirect inspiration:
 - https://github.com/Geal
 - https://github.com/cecton
 - https://fasterthanli.me/articles/request-coalescing-in-async-rust
 - various apollographql router developers

Thanks for the input and good advice. All mistakes/errors are of course mine.

## License

Apache 2.0 licensed. See LICENSE for details.
