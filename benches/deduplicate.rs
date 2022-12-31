use deduplicate::Deduplicate;
use deduplicate::DeduplicateFuture;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use moka::future::Cache;
use rand::{thread_rng, Rng};

// Utility function for loading a vec of strings from disk
// a large file and a small file for different testing styles
fn get_text() -> Vec<String> {
    use std::fs::File;
    use std::io::Read;
    const DATA: &[&str] = &["data/1984.txt", "data/sun-rising.txt"];
    let mut contents = String::new();
    File::open(&DATA[0])
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
        let words = get_text();
        let fut = async move {
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
    // 0% = 0, 5% = 512, 10% = 1024, 20% = 2048, 40% = 4096, 80% = 8192
    for size in [0, 512, 1024, 2048, 4096, 8192].iter() {
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
        eprintln!("deduplicate cache used: {}", deduplicate.count());

        // Benchmark moka
        let moka = Cache::new(*size as u64);
        group.bench_with_input(BenchmarkId::new("moka get", size), &words, |b, words| {
            b.to_async(tokio::runtime::Runtime::new().expect("build tokio runtime"))
                .iter(|| async {
                    let word = &words[thread_rng().gen_range(0..words.len())];
                    let _ = moka
                        .optionally_get_with(word.to_string(), getter(word.to_string()))
                        .await;
                })
        });
        eprintln!("moka cache used: {}", moka.entry_count());
    }
}

criterion_group!(benches, cache_get,);
criterion_main!(benches);
