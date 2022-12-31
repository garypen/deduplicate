use deduplicate::Deduplicate;
use deduplicate::DeduplicateError;
use deduplicate::DeduplicateFuture;

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use moka::future::Cache;
use rand::{
    distributions::{Alphanumeric, Standard},
    thread_rng, Rng,
};

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

fn deduplicate_get(c: &mut Criterion) {
    // Deduplicate Version
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
    let deduplicate = Deduplicate::new(getter);

    let words = get_text();
    let word = &words[thread_rng().gen_range(0..words.len())];

    c.bench_with_input(
        BenchmarkId::new("deduplicate_get (word)", word),
        &word,
        |b, word| {
            b.to_async(tokio::runtime::Runtime::new().expect("build tokio runtime"))
                .iter(|| async {
                    let _ = deduplicate.get(word.to_string()).await;
                })
        },
    );

    // Moka Version
    let moka: Cache<String, String> = Cache::new(512);
    let moka_getter = move |key: String| {
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
    c.bench_with_input(BenchmarkId::new("moka (word)", word), &word, |b, word| {
        b.to_async(tokio::runtime::Runtime::new().expect("build tokio runtime"))
            .iter(|| async {
                let _ = moka
                    .optionally_get_with(word.to_string(), moka_getter(word.to_string()))
                    .await;
            })
    });
}

/*
fn criterion_benchmark(c: &mut Criterion) {
    let mut deduplicate = TrieString::<usize>::new();
    c.bench_function("inserting: char items (len: 1..=512)", |b| {
        b.iter_batched(
            || {
                thread_rng()
                    .sample_iter(&Alphanumeric)
                    .take(thread_rng().gen_range(1..=512))
                    .map(char::from)
            },
            |input| insert_deduplicate(&mut deduplicate, input),
            BatchSize::SmallInput,
        )
    });
    c.bench_function("contains: char items (len: 1..=512)", |b| {
        b.iter_batched(
            || {
                thread_rng()
                    .sample_iter(&Alphanumeric)
                    .take(thread_rng().gen_range(1..=512))
                    .map(char::from)
            },
            |input| contains_deduplicate(&deduplicate, input),
            BatchSize::SmallInput,
        )
    });
    deduplicate.clear();
}

fn iterate(c: &mut Criterion) {
    static BASE_SIZE: usize = 16;
    static POPULATION_SIZE: usize = 1000;

    let mut group = c.benchmark_group("iterate");
    for size in [
        BASE_SIZE,
        2 * BASE_SIZE,
        4 * BASE_SIZE,
        8 * BASE_SIZE,
        16 * BASE_SIZE,
        32 * BASE_SIZE,
        64 * BASE_SIZE,
    ]
    .iter()
    {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(
            BenchmarkId::new("consuming iteration (char)", size),
            size,
            |b, &size| {
                let mut deduplicate = TrieString::<usize>::new();
                for _i in 0..POPULATION_SIZE {
                    let entry: Vec<char> = thread_rng()
                        .sample_iter(&Alphanumeric)
                        .take(thread_rng().gen_range(1..=size))
                        .map(char::from)
                        .collect();
                    deduplicate.insert(entry);
                }
                b.iter_batched(
                    || deduplicate.clone(),
                    iterate_deduplicate,
                    BatchSize::SmallInput,
                )
            },
        );
        group.bench_with_input(
            BenchmarkId::new("reference iteration (char)", size),
            size,
            |b, &size| {
                let mut deduplicate = TrieString::<usize>::new();
                for _i in 0..POPULATION_SIZE {
                    let entry: Vec<char> = thread_rng()
                        .sample_iter(&Alphanumeric)
                        .take(thread_rng().gen_range(1..=size))
                        .map(char::from)
                        .collect();
                    deduplicate.insert(entry);
                }
                b.iter_batched(
                    || {},
                    |_| iterate_deduplicate_ref(&deduplicate),
                    BatchSize::SmallInput,
                )
            },
        );
    }
    group.finish();
}

fn search(c: &mut Criterion) {
    static BASE_SIZE: usize = 16;
    static POPULATION_SIZE: usize = 10000;

    let mut group = c.benchmark_group("search");
    for size in [
        BASE_SIZE,
        2 * BASE_SIZE,
        4 * BASE_SIZE,
        8 * BASE_SIZE,
        16 * BASE_SIZE,
        32 * BASE_SIZE,
        64 * BASE_SIZE,
    ]
    .iter()
    {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(
            BenchmarkId::new("random find (usize)", size),
            size,
            |b, &size| {
                let mut deduplicate = TrieVec::<usize, usize>::new();
                for _i in 0..POPULATION_SIZE {
                    let entry: Vec<usize> = thread_rng()
                        .sample_iter(Standard)
                        .take(thread_rng().gen_range(1..=size))
                        .collect();
                    deduplicate.insert(entry);
                }
                b.iter_batched(
                    || {
                        thread_rng()
                            .sample_iter(Standard)
                            .take(thread_rng().gen_range(1..=size))
                    },
                    |input| contains_deduplicate(&deduplicate, input),
                    BatchSize::SmallInput,
                )
            },
        );
        group.bench_with_input(
            BenchmarkId::new("always find (usize)", size),
            size,
            |b, &size| {
                let mut deduplicate = TrieVec::<usize, usize>::new();
                let mut searches: Vec<Vec<usize>> = vec![];
                for _i in 0..POPULATION_SIZE {
                    let entry: Vec<usize> = thread_rng()
                        .sample_iter(Standard)
                        .take(thread_rng().gen_range(1..=size))
                        .collect();
                    searches.push(entry.clone());
                    deduplicate.insert(entry);
                }
                b.iter_batched(
                    || searches[thread_rng().gen_range(1..POPULATION_SIZE)].clone(),
                    |input| contains_deduplicate(&deduplicate, input),
                    BatchSize::SmallInput,
                )
            },
        );
        group.bench_with_input(
            BenchmarkId::new("random find (char)", size),
            size,
            |b, &size| {
                let mut deduplicate = TrieString::<usize>::new();
                for _i in 0..POPULATION_SIZE {
                    let entry: Vec<char> = thread_rng()
                        .sample_iter(&Alphanumeric)
                        .take(thread_rng().gen_range(1..=size))
                        .map(char::from)
                        .collect();
                    deduplicate.insert(entry);
                }
                b.iter_batched(
                    || {
                        thread_rng()
                            .sample_iter(&Alphanumeric)
                            .take(thread_rng().gen_range(1..=size))
                            .map(char::from)
                    },
                    |input| contains_deduplicate(&deduplicate, input),
                    BatchSize::SmallInput,
                )
            },
        );
        group.bench_with_input(
            BenchmarkId::new("always find (char)", size),
            size,
            |b, &size| {
                let mut deduplicate = TrieString::<usize>::new();
                let mut searches: Vec<Vec<char>> = vec![];
                for _i in 0..POPULATION_SIZE {
                    let entry: Vec<char> = thread_rng()
                        .sample_iter(&Alphanumeric)
                        .take(thread_rng().gen_range(1..=size))
                        .map(char::from)
                        .collect();
                    searches.push(entry.clone());
                    deduplicate.insert(entry);
                }
                b.iter_batched(
                    || searches[thread_rng().gen_range(1..POPULATION_SIZE)].clone(),
                    |input| contains_deduplicate(&deduplicate, input),
                    BatchSize::SmallInput,
                )
            },
        );
    }
    group.finish();
}
*/

criterion_group!(
    benches,
    // deduplicate_insert,
    deduplicate_get,
    // deduplicate_insert_remove,
    // criterion_benchmark,
    // search,
    // iterate
);
criterion_main!(benches);

/*
fn insert_deduplicate<S: IntoIterator<Item = A>, K: TrieKey<A>, A: TrieAtom, V: TrieValue>(
    deduplicate: &mut Trie<K, A, V>,
    input: S,
) {
    deduplicate.insert(input);
}

fn contains_deduplicate<S: IntoIterator<Item = A>, K: TrieKey<A>, A: TrieAtom, V: TrieValue>(
    deduplicate: &Trie<K, A, V>,
    input: S,
) {
    deduplicate.contains(input);
}

fn iterate_deduplicate<K: TrieKey<A>, A: TrieAtom, V: TrieValue>(deduplicate: Trie<K, A, V>) {
    deduplicate.into_iter().for_each(|_x| ());
}

fn iterate_deduplicate_ref<K: TrieKey<A>, A: TrieAtom, V: TrieValue>(deduplicate: &Trie<K, A, V>) {
    deduplicate.iter().for_each(|_x| ());
}
*/
