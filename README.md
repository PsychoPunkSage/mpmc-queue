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
1P x 1C                 5.08e6 ops/sec
4P x 4C                 3.98e6 ops/sec
16P x 16C               3.70e6 ops/sec

=== Capacity sweep (4P x 4C) ===
cap=64                  3.76e6 ops/sec
cap=1024                4.01e6 ops/sec

=== Asymmetric (capacity=256) ===
8P x 2C                 3.42e6 ops/sec
2P x 8C                 3.24e6 ops/sec
```

## Constraints

- `std` only — no external dependencies
- `unsafe` used only where necessary, each site justified in comments
