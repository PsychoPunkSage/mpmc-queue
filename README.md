# mpmc

Bounded multi-producer multi-consumer queue in Rust. Lock-free on the fast path, blocks on full/empty.

## Algorithm

Dmitry Vyukov's ring buffer: each slot has a sequence number that acts as an ownership ticket. Producers and consumers race via CAS; the winner gets exclusive access to the slot. Blocking uses `Mutex` + `Condvar` with a double-check to avoid lost wakeups.

Capacity is rounded up to the next power of two internally.

## Usage

```rust
use mpmc::{BoundedQueue, MpmcQueue};
use std::sync::Arc;

let q = Arc::new(MpmcQueue::<i32>::new(256));

// blocking
q.push(42);
let v = q.pop();

// non-blocking
q.try_push(42).ok();
q.try_pop(); // Option<T>
```

## Tests

```
cargo test
```

## Benchmarks

```
cargo bench
```

Sample output (your numbers will vary):

```
=== Symmetric (capacity=1024) ===
1P x 1C                    5.38e6 ops/sec   (1000000 items, 0.186s)
2P x 2C                    5.84e6 ops/sec   (1000000 items, 0.171s)
4P x 4C                    3.62e6 ops/sec   (1000000 items, 0.276s)
8P x 8C                    3.71e6 ops/sec   (1000000 items, 0.270s)
16P x 16C                  3.66e6 ops/sec   (1000000 items, 0.274s)

=== Capacity sweep (4P x 4C) ===
cap=64                     3.66e6 ops/sec   (1000000 items, 0.274s)
cap=256                    3.74e6 ops/sec   (1000000 items, 0.267s)
cap=1024                   3.70e6 ops/sec   (1000000 items, 0.270s)

=== Asymmetric (capacity=256) ===
8P x 2C                    3.15e6 ops/sec   (1000000 items, 0.317s)
2P x 8C                    3.44e6 ops/sec   (1000000 items, 0.290s)
```

## Constraints

- `std` only — no external dependencies
- `unsafe` used only where necessary, each site justified in comments
