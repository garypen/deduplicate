use std::sync::Arc;
use std::time::Instant;

use deduplicate::Deduplicate;
use deduplicate::DeduplicateFuture;

use rand::Rng;

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

#[tokio::main]
async fn main() {
    let deduplicate = Arc::new(Deduplicate::new(get));

    for _i in 0..5 {
        let mut hdls = vec![];
        let start = Instant::now();
        for _i in 0..100 {
            let my_deduplicate = deduplicate.clone();
            hdls.push(async move {
                let is_ok = my_deduplicate.get(&5).await.is_ok();
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
