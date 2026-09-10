//! Lock-free circular audio buffer for real-time CPAL audio playback.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// Lock-free circular ring buffer storing audio samples (`f32`).
///
/// Ensures real-time audio threads never block on `std::sync::Mutex` locks,
/// eliminating priority inversion and audio glitching during WebRTC audio streaming.
pub struct AudioRingBuffer {
    buffer: Vec<AtomicU32>,
    capacity: usize,
    mask: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
}

unsafe impl Send for AudioRingBuffer {}
unsafe impl Sync for AudioRingBuffer {}

impl AudioRingBuffer {
    /// Create a new `AudioRingBuffer` with requested capacity (rounded up to the next power of 2).
    pub fn new(requested_capacity: usize) -> Self {
        let capacity = requested_capacity.next_power_of_two().max(1024);
        let mask = capacity - 1;
        let mut buffer = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            buffer.push(AtomicU32::new(0));
        }
        Self {
            buffer,
            capacity,
            mask,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Create default lock-free audio buffer with 65,536 sample capacity (~1.36 seconds at 48 kHz).
    pub fn default_capacity() -> Self {
        Self::new(65536)
    }

    /// Return the number of audio samples currently buffered.
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        head.wrapping_sub(tail)
    }

    /// Return `true` if the buffer contains no samples.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Push a single audio sample into the buffer using atomic CAS.
    /// Returns `false` if the buffer is full.
    pub fn push(&self, sample: f32) -> bool {
        loop {
            let head = self.head.load(Ordering::Relaxed);
            let tail = self.tail.load(Ordering::Acquire);
            if head.wrapping_sub(tail) >= self.capacity {
                return false;
            }
            if self
                .head
                .compare_exchange_weak(
                    head,
                    head.wrapping_add(1),
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                self.buffer[head & self.mask].store(sample.to_bits(), Ordering::Release);
                return true;
            }
        }
    }

    /// Push a slice of audio samples into the buffer and enforce low-latency bounds.
    pub fn push_slice(&self, samples: &[f32]) {
        for &s in samples {
            let _ = self.push(s);
        }
        self.cap_latency(4800, 1920);
    }

    /// Pop a single audio sample from the buffer.
    /// Returns `None` if the buffer is empty.
    pub fn pop(&self) -> Option<f32> {
        loop {
            let tail = self.tail.load(Ordering::Relaxed);
            let head = self.head.load(Ordering::Acquire);
            if tail == head {
                return None;
            }
            let bits = self.buffer[tail & self.mask].load(Ordering::Acquire);
            if self
                .tail
                .compare_exchange_weak(
                    tail,
                    tail.wrapping_add(1),
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                return Some(f32::from_bits(bits));
            }
        }
    }

    /// Clear all pending samples from the buffer.
    pub fn clear(&self) {
        let head = self.head.load(Ordering::Acquire);
        self.tail.store(head, Ordering::Release);
    }

    /// Enforce maximum buffer latency. If sample count exceeds `max_samples`, drop oldest backlog down to `target_samples`.
    pub fn cap_latency(&self, max_samples: usize, target_samples: usize) {
        let current_len = self.len();
        if current_len > max_samples {
            let drop_count = current_len.saturating_sub(target_samples);
            let tail = self.tail.load(Ordering::Relaxed);
            self.tail
                .store(tail.wrapping_add(drop_count), Ordering::Release);
        }
    }
}

impl std::fmt::Debug for AudioRingBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioRingBuffer")
            .field("capacity", &self.capacity)
            .field("len", &self.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_push_pop_clear() {
        let ring = AudioRingBuffer::new(1024);
        assert!(ring.is_empty());
        assert_eq!(ring.len(), 0);

        assert!(ring.push(0.123));
        assert!(ring.push(0.456));
        assert_eq!(ring.len(), 2);
        assert!(!ring.is_empty());

        assert_eq!(ring.pop(), Some(0.123));
        assert_eq!(ring.pop(), Some(0.456));
        assert_eq!(ring.pop(), None);
        assert!(ring.is_empty());

        ring.push(0.789);
        assert_eq!(ring.len(), 1);
        ring.clear();
        assert_eq!(ring.len(), 0);
        assert_eq!(ring.pop(), None);
    }

    #[test]
    fn test_ring_buffer_slice_and_latency_capping() {
        let ring = AudioRingBuffer::new(1024);
        let samples: Vec<f32> = (0..500).map(|i| i as f32).collect();
        ring.push_slice(&samples);
        assert_eq!(ring.len(), 500);

        ring.cap_latency(400, 100);
        assert_eq!(ring.len(), 100);
        assert_eq!(ring.pop(), Some(400.0));
    }
}
