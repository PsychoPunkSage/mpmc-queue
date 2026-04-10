use mpmc::{BoundedQueue, MpmcQueue};
use std::collections::HashSet;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

#[test]
fn basic_push_pop() {
    let q = MpmcQueue::<i32>::new(4);
    assert!(q.try_push(42).is_ok());
    assert_eq!(q.try_pop(), Some(42));
    assert_eq!(q.try_pop(), None);
}

#[test]
fn fifo_spsc() {
    const N: usize = 1000;
    let q = Arc::new(MpmcQueue::<usize>::new(64));
    let q2 = Arc::clone(&q);

    let producer = thread::spawn(move || {
        for i in 0..N {
            q2.push(i);
        }
    });

    let mut received = Vec::with_capacity(N);
    for _ in 0..N {
        received.push(q.pop());
    }

    producer.join().unwrap();
    assert_eq!(received, (0..N).collect::<Vec<_>>());
}

#[test]
fn try_push_full() {
    let q = MpmcQueue::<i32>::new(2);
    assert!(q.try_push(1).is_ok());
    assert!(q.try_push(2).is_ok());
    assert_eq!(q.try_push(3), Err(3));
}

#[test]
fn try_pop_empty() {
    let q = MpmcQueue::<i32>::new(4);
    assert_eq!(q.try_pop(), None);
}

#[test]
fn capacity_one() {
    let q = Arc::new(MpmcQueue::<i32>::new(1));
    let q2 = Arc::clone(&q);

    q.push(1);

    let producer = thread::spawn(move || {
        q2.push(2); // blocks until slot is free
    });

    thread::sleep(Duration::from_millis(20));
    assert_eq!(q.pop(), 1);

    producer.join().unwrap();
    assert_eq!(q.pop(), 2);
}

#[test]
fn mpmc_no_loss() {
    const PRODUCERS: usize = 4;
    const ITEMS_PER_PROD: usize = 500;
    const TOTAL: usize = PRODUCERS * ITEMS_PER_PROD;

    let q = Arc::new(MpmcQueue::<usize>::new(TOTAL));

    let producers: Vec<_> = (0..PRODUCERS)
        .map(|p| {
            let q = Arc::clone(&q);
            thread::spawn(move || {
                for i in (p * ITEMS_PER_PROD)..((p + 1) * ITEMS_PER_PROD) {
                    q.push(i);
                }
            })
        })
        .collect();

    for h in producers {
        h.join().unwrap();
    }

    // Single-threaded drain after producers finish keeps the check simple.
    let mut all = Vec::new();
    while all.len() < TOTAL {
        if let Some(v) = q.try_pop() {
            all.push(v);
        } else {
            thread::yield_now();
        }
    }

    let set: HashSet<usize> = all.iter().copied().collect();
    assert_eq!(set.len(), TOTAL, "duplicates detected");
    for i in 0..TOTAL {
        assert!(set.contains(&i), "missing item {i}");
    }
}

#[test]
fn mpmc_stress() {
    const PRODUCERS: usize = 16;
    const CONSUMERS: usize = 16;
    const ITEMS_PER_PROD: usize = 1000;

    let q = Arc::new(MpmcQueue::<usize>::new(1024));

    let producers: Vec<_> = (0..PRODUCERS)
        .map(|p| {
            let q = Arc::clone(&q);
            thread::spawn(move || {
                for i in (p * ITEMS_PER_PROD)..((p + 1) * ITEMS_PER_PROD) {
                    q.push(i);
                }
            })
        })
        .collect();

    let consumers: Vec<_> = (0..CONSUMERS)
        .map(|_| {
            let q = Arc::clone(&q);
            thread::spawn(move || {
                for _ in 0..ITEMS_PER_PROD {
                    q.pop();
                }
            })
        })
        .collect();

    for h in producers { h.join().unwrap(); }
    for h in consumers { h.join().unwrap(); }
}

#[test]
fn asymmetric_many_prod() {
    const PRODUCERS: usize = 8;
    const ITEMS_PER_PROD: usize = 200;
    const TOTAL: usize = PRODUCERS * ITEMS_PER_PROD;

    let q = Arc::new(MpmcQueue::<usize>::new(TOTAL));

    let producers: Vec<_> = (0..PRODUCERS)
        .map(|p| {
            let q = Arc::clone(&q);
            thread::spawn(move || {
                for i in (p * ITEMS_PER_PROD)..((p + 1) * ITEMS_PER_PROD) {
                    q.push(i);
                }
            })
        })
        .collect();

    for h in producers { h.join().unwrap(); }

    let mut all = Vec::new();
    while all.len() < TOTAL {
        if let Some(v) = q.try_pop() { all.push(v); } else { thread::yield_now(); }
    }

    let set: HashSet<usize> = all.iter().copied().collect();
    assert_eq!(set.len(), TOTAL);
    for i in 0..TOTAL { assert!(set.contains(&i)); }
}

#[test]
fn asymmetric_many_cons() {
    const PRODUCERS: usize = 2;
    const ITEMS_PER_PROD: usize = 500;
    const TOTAL: usize = PRODUCERS * ITEMS_PER_PROD;

    let q = Arc::new(MpmcQueue::<usize>::new(TOTAL));

    let producers: Vec<_> = (0..PRODUCERS)
        .map(|p| {
            let q = Arc::clone(&q);
            thread::spawn(move || {
                for i in (p * ITEMS_PER_PROD)..((p + 1) * ITEMS_PER_PROD) {
                    q.push(i);
                }
            })
        })
        .collect();

    for h in producers { h.join().unwrap(); }

    let mut all = Vec::new();
    while all.len() < TOTAL {
        if let Some(v) = q.try_pop() { all.push(v); } else { thread::yield_now(); }
    }

    let set: HashSet<usize> = all.iter().copied().collect();
    assert_eq!(set.len(), TOTAL);
    for i in 0..TOTAL { assert!(set.contains(&i)); }
}

#[test]
fn blocking_wakeup() {
    let q = Arc::new(MpmcQueue::<i32>::new(2));

    q.push(1);
    q.push(2);

    let q2 = Arc::clone(&q);
    let producer = thread::spawn(move || {
        q2.push(3); // blocks — queue is full
    });

    thread::sleep(Duration::from_millis(100));
    let v = q.pop();
    assert!(v == 1 || v == 2);

    producer.join().unwrap();
    while q.try_pop().is_some() {}
}

#[test]
fn drop_drains_queue() {
    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    struct Tracked(#[allow(dead_code)] usize);
    impl Drop for Tracked {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::Relaxed);
        }
    }

    DROP_COUNT.store(0, Ordering::Relaxed);
    {
        let q = MpmcQueue::<Tracked>::new(8);
        for i in 0..5 { q.push(Tracked(i)); }
    }
    assert_eq!(DROP_COUNT.load(Ordering::Relaxed), 5);
}
