use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};

use crate::BoundedQueue;

// Pad to a cache line to avoid false sharing.
#[repr(align(64))]
struct CacheAligned<T>(T);

// Each slot holds a sequence number and the item data.
// sequence protocol:
//   init      = slot index i
//   producer writes at pos  -> sequence = pos + 1
//   consumer reads  at pos  -> sequence = pos + capacity
// Cell intentionally has no Drop impl; the queue drains items manually.
#[repr(align(64))]
struct Cell<T> {
    sequence: AtomicUsize,
    data: UnsafeCell<MaybeUninit<T>>,
}

pub struct MpmcQueue<T> {
    buffer: Box<[Cell<T>]>,
    mask: usize,
    enqueue_pos: CacheAligned<AtomicUsize>,
    dequeue_pos: CacheAligned<AtomicUsize>,
    mutex: Mutex<()>,
    not_empty: Condvar,
    not_full: Condvar,
}

// SAFETY: slot access is gated by the sequence-number CAS; T: Send lets
// values cross thread boundaries safely.
unsafe impl<T: Send> Send for MpmcQueue<T> {}
unsafe impl<T: Send> Sync for MpmcQueue<T> {}

impl<T: Send> BoundedQueue<T> for MpmcQueue<T> {
    fn new(capacity: usize) -> Self {
        assert!(capacity >= 1);
        // Minimum 2: with cap=1 the recycled sequence (pos+cap) collides with
        // the next-write sequence (pos+1), breaking the state machine.
        let cap = capacity.next_power_of_two().max(2);
        let buffer = (0..cap)
            .map(|i| Cell {
                sequence: AtomicUsize::new(i),
                data: UnsafeCell::new(MaybeUninit::uninit()),
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        MpmcQueue {
            buffer,
            mask: cap - 1,
            enqueue_pos: CacheAligned(AtomicUsize::new(0)),
            dequeue_pos: CacheAligned(AtomicUsize::new(0)),
            mutex: Mutex::new(()),
            not_empty: Condvar::new(),
            not_full: Condvar::new(),
        }
    }

    fn try_push(&self, item: T) -> Result<(), T> {
        loop {
            let pos = self.enqueue_pos.0.load(Ordering::Relaxed);
            let cell = &self.buffer[pos & self.mask];
            let seq = cell.sequence.load(Ordering::Acquire);
            let diff = seq as isize - pos as isize;

            if diff == 0 {
                match self.enqueue_pos.0.compare_exchange_weak(
                    pos,
                    pos.wrapping_add(1),
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        // SAFETY: CAS win gives exclusive ownership of this slot.
                        // write() skips Drop on the uninitialized target.
                        unsafe { (*cell.data.get()).write(item) };
                        cell.sequence.store(pos.wrapping_add(1), Ordering::Release);
                        return Ok(());
                    }
                    Err(_) => continue,
                }
            } else if diff < 0 {
                return Err(item); // full
            }
            // diff > 0: stale pos, retry
        }
    }

    fn try_pop(&self) -> Option<T> {
        loop {
            let pos = self.dequeue_pos.0.load(Ordering::Relaxed);
            let cell = &self.buffer[pos & self.mask];
            let seq = cell.sequence.load(Ordering::Acquire);
            let diff = seq as isize - pos.wrapping_add(1) as isize;

            if diff == 0 {
                match self.dequeue_pos.0.compare_exchange_weak(
                    pos,
                    pos.wrapping_add(1),
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        // SAFETY: CAS win gives exclusive ownership; Acquire on
                        // sequence syncs with the producer's Release, so data is
                        // initialized. assume_init_read() moves without dropping.
                        let item = unsafe { (*cell.data.get()).assume_init_read() };
                        cell.sequence
                            .store(pos.wrapping_add(self.buffer.len()), Ordering::Release);
                        return Some(item);
                    }
                    Err(_) => continue,
                }
            } else if diff < 0 {
                return None; // empty
            }
            // diff > 0: stale pos, retry
        }
    }

    fn push(&self, item: T) {
        let mut item = item;
        loop {
            match self.try_push(item) {
                Ok(()) => { self.not_empty.notify_one(); return; }
                Err(ret) => item = ret,
            }
            // Re-check under the lock to avoid a lost wakeup between the
            // failed try_push and the condvar wait.
            let guard = self.mutex.lock().unwrap();
            match self.try_push(item) {
                Ok(()) => { drop(guard); self.not_empty.notify_one(); return; }
                Err(ret) => item = ret,
            }
            let _g = self.not_full.wait(guard).unwrap();
        }
    }

    fn pop(&self) -> T {
        loop {
            if let Some(item) = self.try_pop() {
                self.not_full.notify_one();
                return item;
            }
            let guard = self.mutex.lock().unwrap();
            if let Some(item) = self.try_pop() {
                drop(guard);
                self.not_full.notify_one();
                return item;
            }
            let _g = self.not_empty.wait(guard).unwrap();
        }
    }
}

impl<T> Drop for MpmcQueue<T> {
    fn drop(&mut self) {
        // MaybeUninit won't auto-drop; walk live slots and drop each item.
        let mut pos = self.dequeue_pos.0.load(Ordering::Relaxed);
        loop {
            let cell = &self.buffer[pos & self.mask];
            let seq = cell.sequence.load(Ordering::Relaxed);
            if seq as isize - pos.wrapping_add(1) as isize == 0 {
                // SAFETY: &mut self => exclusive access; seq==pos+1 proves slot
                // is initialized. Value is dropped when _item goes out of scope.
                let _item = unsafe { (*cell.data.get()).assume_init_read() };
                pos = pos.wrapping_add(1);
            } else {
                break;
            }
        }
    }
}
