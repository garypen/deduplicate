# deduplicate
asynchronous deduplicator with optional LRU caching

If you have "slow", "expensive" or "flaky" tasks for which you'd like to provide de-duplication and, optionally, result caching, then this may be the crate you are looking for.

The `Deduplicate` struct controls concurrent access to data via a delegated function provided when you create your `Deduplicate` instance.

```
use std::sync::Arc;
use std::time::Instant;

use deduplicate::Deduplicate;
use deduplicate::DeduplicateFuture;

use rand::Rng;

/// If our delegated getter panics, all our concurrent gets will
/// fail. Let's cause that to happen sometimes by panicking on even
/// numbers.
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

/// Create our deduplicate and then loop around 5 times creating 100
/// jobs which all call our delegated get function.
/// We print out data about each iteration where we see how many
/// succeed, the range of times between each invocation, the set
/// of results and how long the iteration took.
/// The results of running this will vary depending on whether or not
/// our random number generator provides us with an even number.
/// As long as we get even numbers, all of our gets will fail and
/// the delegated get will continue to be invoked. As soon as we
/// get a delegated call that succeeds, all of our remaing loops
/// will succeed since they'll get the value from the cache.
#[tokio::main]
async fn main() {
    let deduplicate = Arc::new(Deduplicate::new(get));

    for _i in 0..5 {
        let mut hdls = vec![];
        let start = Instant::now();
        for _i in 0..100 {
            let my_deduplicate = deduplicate.clone();
            hdls.push(async move {
                let is_ok = my_deduplicate.get(5).await.is_ok();
                (Instant::now(), is_ok)
            });
        }
        let mut result: Vec<(Instant, bool)> =
            futures::future::join_all(hdls).await.into_iter().collect();
        result.sort();
        println!(
            "range: {:?}",
            result.last().unwrap().0 - result.first().unwrap().0
        );
        println!(
            "passed: {:?}",
            result
                .iter()
                .fold(0, |acc, x| if x.1 { acc + 1 } else { acc })
        );
        println!("result: {:?}", result);
        println!("elapsed: {:?}\n", Instant::now() - start);
    }
}
```

[![Crates.io](https://img.shields.io/crates/v/deduplicate.svg)](https://crates.io/crates/deduplicate)

[API Docs](https://docs.rs/deduplicate/latest/deduplicate)

## Installation

```toml
[dependencies]
deduplicate = "0.4"
```

## Acknowledgements

This crate build upon the hard work and inspiration of several folks, some of whom I have worked with directly and some from whom I have taken indirect inspiration:
 - https://github.com/Geal
 - https://github.com/cecton
 - https://fasterthanli.me/articles/request-coalescing-in-async-rust
 - various apollographql router developers

Thanks for the input and good advice. All mistakes/errors are of course mine.

## Benchmarks

SEE THE UPDATE BELOW FOR AN UPDATED COMPARISON

When I announced this crate on [reddit](https://www.reddit.com/r/rust/comments/yt9yaz/caching_asynchronous_request_deduplication/), the main feedback that I got was that I should do some re-working to examine removing the Mutex that is used to control access to the internal WaitMap. Rather than just go ahead and do this, I thought I'd do some comparisons with `moka` to see how the current performance compares.

I like `moka` a lot, it's got a huge amount of functionality and is very good across a wide range of problem applications. However, for the specific scenarios that I benchmarked, I found `deduplicate` was more performant than `moka` even with the Mutex in place.

My benchmarking was against a very specific problem set and there's no guarantee that `deduplicate` is always faster than `moka`. All my tests were performed on an AMD Ryzen 7 3700X 8-Core running ubuntu 20.04 with rustc 1.66.0. The comparison was between `deduplicate` 0.3.2 and `moka` 0.9.6.

The benchmark is highly concurrent and involves a O(n) search across a small (approx 10,000 entries) dataset of strings (loaded from disk) whenever there is a cache miss. `deduplicate` only provides an asynchronous interface, so the comparison is against the moka future::Cache cache.

(I've removed the links to the old data, since they are not representative of the current state of things. See the update below for the latest results. I'll leave my speculation and thoughts here to provide some context for the update.)

I was quite surprised with the `moka` results. The performance didn't seem to improve when I varied the size of the cache. `deduplicate` improved with more cache until it had a cache which was sized at about 80% of the master dataset. That's what I would have expected from `moka` as well, although perhaps the source of this unexpected behaviour is the "batching" which `moka` performs under concurrent load. It's also possible that I've not written my benchmarking code correctly, but I've tried to be consistent between the two crates, so please let me know if you spot any errors I can correct.

I'll put some thought into ways to remove the Mutex and maintain performance when I have more spare cycles.

### Update (11/02/2023)

The primary `moka` author raised an [issue](https://github.com/garypen/deduplicate/issues/1) against the benchmarks (thank you!) and provided an explanation as to why `moka` did not perform as well as I expected.

I've merged the PR to update the benchmarks and generated a new set that show much better results for `moka`.

I generated the updated results on the same system, with a newer rust compiler (1.67.1) and comparing `deduplicate` (0.3.6) with `moka` (0.10.0).

If you want to look at my criterion generated [report](https://garypen.github.io/deduplicate/target/criterion/report/index.html), the clearest comparison is from clicking on the `get` link, but feel free to dig into the details.

If you want to generate your own set of benchmarking comparisons, download the repo and run the following:

```
cargo bench --bench deduplicate -- --plotting-backend gnuplot --baseline 0.3.6
```

This assumes that you have gnuplot installed on your system. (`apt install gnuplot`) and that you have installed [criterion](https://crates.io/crates/cargo-criterion) for benchmarking.

If I had to draw any conclusions from the updated reports, I'd say there's not much performance difference, but `moka` seems to perform better when the cache is limited to a small % of the total number of possible results with `deduplicate` performing better as the size of the cache increases.

It's probably worth looking at the mutex now... :)

## License

Apache 2.0 licensed. See LICENSE for details.
