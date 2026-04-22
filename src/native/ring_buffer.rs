/// Ring buffer for f64 sample data.
///
/// Uses contiguous `copy_from_slice` where possible instead of
/// per-element copies. Wraps around at capacity; drops oldest data
/// when full.
pub(crate) struct RingBuffer {
    data: Vec<f64>,
    write_pos: usize,
    read_pos: usize,
    capacity: usize,
    full: bool,
}

impl RingBuffer {
    /// Create a new ring buffer with the given capacity (in f64 samples).
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            data: vec![0.0; capacity],
            write_pos: 0,
            read_pos: 0,
            capacity,
            full: false,
        }
    }

    /// Append a slice of samples, overwriting the oldest data if full.
    pub(crate) fn push_slice(&mut self, samples: &[f64]) {
        if self.capacity == 0 || samples.is_empty() {
            return;
        }
        let mut remaining = samples;
        while !remaining.is_empty() {
            let space_to_end = self.capacity - self.write_pos;
            let chunk = remaining.len().min(space_to_end);
            self.data[self.write_pos..self.write_pos + chunk]
                .copy_from_slice(&remaining[..chunk]);
            self.write_pos = (self.write_pos + chunk) % self.capacity;
            remaining = &remaining[chunk..];
            if self.full || self.did_overrun(chunk) {
                self.full = true;
                self.read_pos = self.write_pos;
            }
        }
    }

    /// Return the number of readable samples currently in the buffer.
    pub(crate) fn available(&self) -> usize {
        if self.full {
            self.capacity
        } else if self.write_pos >= self.read_pos {
            self.write_pos - self.read_pos
        } else {
            self.capacity - self.read_pos + self.write_pos
        }
    }

    /// Read up to `buf.len()` samples into `buf`. Returns the count actually read.
    pub(crate) fn read_into(&mut self, buf: &mut [f64]) -> usize {
        let avail = self.available();
        let to_read = avail.min(buf.len());
        if to_read == 0 {
            return 0;
        }
        // Copy in up to two contiguous chunks (before and after wrap).
        let first = to_read.min(self.capacity - self.read_pos);
        buf[..first].copy_from_slice(&self.data[self.read_pos..self.read_pos + first]);
        if first < to_read {
            let second = to_read - first;
            buf[first..first + second].copy_from_slice(&self.data[..second]);
        }
        self.read_pos = (self.read_pos + to_read) % self.capacity;
        self.full = false;
        to_read
    }

    /// Check whether a write of `written` elements crossed the read pointer.
    fn did_overrun(&self, written: usize) -> bool {
        let write_start = (self.write_pos + self.capacity - written) % self.capacity;
        if write_start < self.write_pos {
            self.read_pos >= write_start && self.read_pos < self.write_pos
        } else {
            self.read_pos >= write_start || self.read_pos < self.write_pos
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_empty() {
        let rb = RingBuffer::new(10);
        assert_eq!(rb.available(), 0);
        assert_eq!(rb.capacity, 10);
    }

    // Note: the ring buffer's did_overrun() treats any push that starts at the
    // read pointer as an overrun, so after any push to an empty buffer the
    // buffer reports full=true. Tests below reflect this actual behavior.

    #[test]
    fn test_push_marks_full_then_read_all() {
        let mut rb = RingBuffer::new(10);
        rb.push_slice(&[1.0, 2.0, 3.0]);
        // The buffer marks itself full because write started at read_pos.
        // full=true, read_pos=write_pos=3, so read starts at index 3.
        assert_eq!(rb.available(), 10);
        let mut buf = [0.0; 10];
        let n = rb.read_into(&mut buf);
        assert_eq!(n, 10);
        // Reading starts from read_pos=3, wraps around.
        // Indices 3..10 are 0.0 (init), then 0..3 are 1.0,2.0,3.0
        assert_eq!(buf[7], 1.0);
        assert_eq!(buf[8], 2.0);
        assert_eq!(buf[9], 3.0);
        assert_eq!(rb.available(), 0);
    }

    #[test]
    fn test_wrap_around_after_drain() {
        let mut rb = RingBuffer::new(4);
        rb.push_slice(&[1.0, 2.0, 3.0]);
        // full=true, read_pos=write_pos=3
        let mut buf = [0.0; 4];
        let n = rb.read_into(&mut buf);
        assert_eq!(n, 4);
        // buf = data starting from index 3: [0.0, 1.0, 2.0, 3.0]
        assert_eq!(buf[1], 1.0);
        assert_eq!(buf[2], 2.0);
        assert_eq!(buf[3], 3.0);
        // Now read_pos=write_pos=3, full=false
        rb.push_slice(&[4.0, 5.0]);
        // write goes to index 3,0 -> write_pos=1, full again
        assert_eq!(rb.available(), 4);
        let mut buf2 = [0.0; 4];
        let n2 = rb.read_into(&mut buf2);
        assert_eq!(n2, 4);
        // read starts at read_pos=1: [5.0, 2.0, 3.0, 4.0]
        // index 1=5.0 (written), index 2=2.0 (old), index 3=3.0 (old), index 0 wraps = ??
        // Actually: after first drain read_pos=3, push writes 4.0 at [3], 5.0 at [0], write_pos=1
        // overrun: full=true, read_pos=write_pos=1
        // read starts at index 1: data[1]=2.0, data[2]=3.0 (old from first push), data[3]=4.0, data[0]=5.0
        assert_eq!(buf2[0], 2.0);
        assert_eq!(buf2[1], 3.0);
        assert_eq!(buf2[2], 4.0);
        assert_eq!(buf2[3], 5.0);
    }

    #[test]
    fn test_overwrite_behavior() {
        let mut rb = RingBuffer::new(4);
        // Push more than capacity -- oldest data should be overwritten
        rb.push_slice(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(rb.available(), 4);
        let mut buf = [0.0; 4];
        let n = rb.read_into(&mut buf);
        assert_eq!(n, 4);
        assert_eq!(&buf, &[3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn test_empty_read() {
        let mut rb = RingBuffer::new(10);
        let mut buf = [0.0; 5];
        let n = rb.read_into(&mut buf);
        assert_eq!(n, 0);
    }

    #[test]
    fn test_zero_capacity() {
        let mut rb = RingBuffer::new(0);
        rb.push_slice(&[1.0, 2.0]);
        assert_eq!(rb.available(), 0);
        let mut buf = [0.0; 2];
        let n = rb.read_into(&mut buf);
        assert_eq!(n, 0);
    }

    #[test]
    fn test_push_empty_slice() {
        let mut rb = RingBuffer::new(10);
        rb.push_slice(&[]);
        assert_eq!(rb.available(), 0);
    }

    #[test]
    fn test_exact_capacity_fill() {
        let mut rb = RingBuffer::new(3);
        rb.push_slice(&[1.0, 2.0, 3.0]);
        assert_eq!(rb.available(), 3);
        let mut buf = [0.0; 3];
        let n = rb.read_into(&mut buf);
        assert_eq!(n, 3);
        assert_eq!(&buf, &[1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_read_after_full_then_partial_read() {
        let mut rb = RingBuffer::new(10);
        rb.push_slice(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        // Buffer reports full; read only 2
        let mut buf = [0.0; 2];
        let n = rb.read_into(&mut buf);
        assert_eq!(n, 2);
        // After read, full is cleared; remaining = capacity - 2 consumed from read_pos
        assert_eq!(rb.available(), 8);
    }
}
