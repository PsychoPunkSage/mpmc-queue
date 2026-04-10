use mpmc::{BoundedQueue, MpmcQueue};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;

fn bench(label: &str, producers: usize, consumers: usize, capacity: usize, total_items: usize) {
    let items_per_producer = total_items / producers;
    let actual_total = items_per_producer * producers;
    let items_per_consumer = actual_total / consumers;

    let q = Arc::new(MpmcQueue::<usize>::new(capacity));
    let barrier = Arc::new(Barrier::new(producers + consumers + 1));

    let prod_handles: Vec<_> = (0..producers)
        .map(|p| {
            let q = Arc::clone(&q);
            let b = Arc::clone(&barrier);
            thread::spawn(move || {
                b.wait();
                for i in (p * items_per_producer)..((p + 1) * items_per_producer) {
                    q.push(i);
                }
            })
        })
        .collect();

    let cons_handles: Vec<_> = (0..consumers)
        .map(|_| {
            let q = Arc::clone(&q);
            let b = Arc::clone(&barrier);
            thread::spawn(move || {
                b.wait();
                for _ in 0..items_per_consumer {
                    q.pop();
                }
            })
        })
        .collect();

    barrier.wait();
    let start = Instant::now();

    for h in prod_handles { h.join().unwrap(); }
    for h in cons_handles { h.join().unwrap(); }

    let elapsed = start.elapsed();
    println!(
        "{:<22} {:>10.2e} ops/sec   ({} items, {:.3}s)",
        label,
        actual_total as f64 / elapsed.as_secs_f64(),
        actual_total,
        elapsed.as_secs_f64()
    );
}

fn main() {
    // total must be divisible by all consumer counts used below (1,2,4,8,16)
    let total = 1_000_000;

    println!("=== Symmetric (capacity=1024) ===");
    for n in [1usize, 2, 4, 8, 16] {
        bench(&format!("{n}P x {n}C"), n, n, 1024, total);
    }

    println!("\n=== Capacity sweep (4P x 4C) ===");
    for cap in [64usize, 256, 1024] {
        bench(&format!("cap={cap}"), 4, 4, cap, total);
    }

    println!("\n=== Asymmetric (capacity=256) ===");
    bench("8P x 2C", 8, 2, 256, total);
    bench("2P x 8C", 2, 8, 256, total);
}
