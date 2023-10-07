use std::sync::{
    atomic::{self, AtomicU64},
    Arc,
};

use deduplicate::Deduplicate;
use deduplicate::DeduplicateFuture;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use moka::future::{Cache};
use rand::{thread_rng, Rng};

// Utility function for loading a vec of strings from disk
// a large file and a small file for different testing styles
fn get_text() -> Vec<String> {
    use std::fs::File;
    use std::io::Read;
    const DATA: &[&str] = &["data/1984.txt", "data/sun-rising.txt"];
    let mut contents = String::new();
    File::open(DATA[0])
        .unwrap()
        .read_to_string(&mut contents)
        .unwrap();
    contents
        .split(|c: char| c.is_whitespace())
        .map(|s| s.to_string())
        .collect()
}

fn cache_get(c: &mut Criterion) {
    // Shared getter for both deduplicate and moka
    // It's deliberately slow (loads words every time it's called)
    // to illustrate that missing caches is expensive...
    let getter = move |key: String| -> DeduplicateFuture<String> {
        let fut = async move {
            let words = get_text();
            for word in words {
                if key == *word {
                    return Some("Found It".to_string());
                }
            }
            None
        };
        Box::pin(fut)
    };

    // Shared source dictionary (contains approx 10,000 words)
    let words = get_text();

    let mut group = c.benchmark_group("get");
    // Approx max cache size as % total amount of data
    // 0% = 0, 0.6% = 64, 1.25% = 128, 2.5% = 256, 5% = 512, 10% = 1024, 20% = 2048, 40% = 4096, 80% = 8192
    for size in [0, 64, 128, 256, 512, 1024, 2048, 4096, 8192].iter() {
        // Benchmark deduplicate
        let deduplicate = Deduplicate::with_capacity(getter, *size);
        group.bench_with_input(
            BenchmarkId::new("deduplicate get", size),
            &words,
            |b, words| {
                b.to_async(tokio::runtime::Runtime::new().expect("build tokio runtime"))
                    .iter(|| async {
                        let word = &words[thread_rng().gen_range(0..words.len())];
                        let _ = deduplicate.get(word.to_string()).await;
                    })
            },
        );
        eprintln!(
            "deduplicate cache used - count: {}, get_count: {}, hit_ratio: {:.2}%",
            deduplicate.count(),
            deduplicate.request_count(),
            deduplicate.request_deduplicated_count() as f64 / deduplicate.request_count() as f64
                * 100.0,
        );

        // Benchmark moka
        let moka = Cache::new(*size as u64);
        let get_count = Arc::new(AtomicU64::default());
        let hit_count = Arc::new(AtomicU64::default());
        group.bench_with_input(BenchmarkId::new("moka get", size), &words, |b, words| {
            b.to_async(tokio::runtime::Runtime::new().expect("build tokio runtime"))
                .iter(|| async {
                    let word = &words[thread_rng().gen_range(0..words.len())];
                    let maybe_entry = moka
                        .entry_by_ref(word)
                        .or_optionally_insert_with(getter(word.to_string()))
                        .await;
                    match maybe_entry {
                        Some(entry) if !entry.is_fresh() => {
                            // Hit
                            hit_count.fetch_add(1, atomic::Ordering::AcqRel);
                        }
                        None | Some(_) => { // Miss
                        }
                    }
                    get_count.fetch_add(1, atomic::Ordering::AcqRel);

                    // Process pending evictions. Without this, moka will hold more
                    // entries than the max capacity, which will skew the benchmarking
                    // result by increasing the hit ratio.
                    moka.run_pending_tasks().await;
                })
        });

        let get_count = get_count.load(atomic::Ordering::Acquire);
        let hit_count = hit_count.load(atomic::Ordering::Acquire);

        eprintln!(
            "moka cache used - entry_count: {}, get_count: {}, hit_ratio: {:.2}%",
            moka.entry_count(),
            get_count,
            hit_count as f64 / get_count as f64 * 100.0,
        );
    }
}

criterion_group!(benches, cache_get,);
criterion_main!(benches);
