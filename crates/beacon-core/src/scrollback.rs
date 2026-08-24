use std::collections::VecDeque;

/// Bytes retained per session so a panel can be rebuilt without asking the
/// process to repeat itself.
///
/// A quarter of a megabyte is a few thousand lines of typical output — enough
/// to switch away and back without losing context, small enough that dozens of
/// sessions cost nothing worth measuring.
pub const DEFAULT_CAPACITY: usize = 256 * 1024;

/// A fixed-size ring of recent PTY output.
///
/// Deliberately byte-oriented rather than line-oriented: this data is a stream
/// of terminal escape sequences, and splitting it on newlines would corrupt any
/// sequence that straddles the boundary.
#[derive(Debug)]
pub struct Scrollback {
    bytes: VecDeque<u8>,
    capacity: usize,
    /// Total bytes ever pushed, including those already evicted.
    ///
    /// This is what lets a client replay a snapshot and then join the live
    /// stream without dropping or duplicating a single byte.
    written: u64,
}

impl Scrollback {
    pub fn new(capacity: usize) -> Self {
        Self {
            bytes: VecDeque::with_capacity(capacity.min(64 * 1024)),
            capacity,
            written: 0,
        }
    }

    /// Appends a chunk and returns the stream offset of its first byte.
    pub fn push(&mut self, chunk: &[u8]) -> u64 {
        let offset = self.written;
        self.written += chunk.len() as u64;

        // A chunk larger than the whole buffer can only contribute its tail.
        if chunk.len() >= self.capacity {
            self.bytes.clear();
            self.bytes.extend(&chunk[chunk.len() - self.capacity..]);
            return offset;
        }

        let overflow = (self.bytes.len() + chunk.len()).saturating_sub(self.capacity);
        self.bytes.drain(..overflow);
        self.bytes.extend(chunk);
        offset
    }

    /// The retained bytes, together with the stream offset just past them.
    ///
    /// A client writes the bytes, then ignores any live chunk that ends at or
    /// before this offset.
    pub fn snapshot(&self) -> (Vec<u8>, u64) {
        (self.bytes.iter().copied().collect(), self.written)
    }

    pub fn clear(&mut self) {
        self.bytes.clear();
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

impl Default for Scrollback {
    fn default() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_everything_while_under_capacity() {
        let mut buffer = Scrollback::new(16);
        buffer.push(b"hello ");
        buffer.push(b"world");
        assert_eq!(buffer.snapshot().0, b"hello world");
    }

    #[test]
    fn drops_the_oldest_bytes_once_full() {
        let mut buffer = Scrollback::new(8);
        buffer.push(b"aaaaaaaa");
        buffer.push(b"bcd");
        assert_eq!(buffer.snapshot().0, b"aaaaabcd");
        assert_eq!(buffer.len(), 8);
    }

    #[test]
    fn a_chunk_larger_than_capacity_keeps_only_its_tail() {
        let mut buffer = Scrollback::new(4);
        buffer.push(b"0123456789");
        assert_eq!(buffer.snapshot().0, b"6789");
    }

    #[test]
    fn escape_sequences_survive_being_pushed_in_pieces() {
        let mut buffer = Scrollback::new(64);
        buffer.push(b"\x1b[3");
        buffer.push(b"1mred\x1b[0m");
        assert_eq!(buffer.snapshot().0, b"\x1b[31mred\x1b[0m");
    }

    #[test]
    fn offsets_keep_counting_past_evicted_bytes() {
        let mut buffer = Scrollback::new(4);
        assert_eq!(buffer.push(b"abcd"), 0);
        assert_eq!(buffer.push(b"efgh"), 4);

        let (bytes, end) = buffer.snapshot();
        assert_eq!(bytes, b"efgh");
        // The offset reflects everything ever written, not what is retained.
        assert_eq!(end, 8);
    }
}
