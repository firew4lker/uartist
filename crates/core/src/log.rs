//! The data log: every chunk sent or received, in order, with its time.
//!
//! Views are rendered from this log, so changing a display option never
//! loses data (`docs/spec/views.md`).
//!
//! The log has a size limit. When it's exceeded, the oldest quarter is
//! dropped in one go, so views only need rebuilding now and then.

use std::time::{Instant, SystemTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Rx,
    Tx,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub dir: Direction,
    pub at: Instant,
    pub data: Vec<u8>,
}

/// Counts how often one chosen byte value is received (HTerm's "Count").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ByteCounter {
    pub value: u8,
    pub count: u64,
}

/// Default memory budget for the log (data plus per-chunk bookkeeping).
pub const DEFAULT_LIMIT_BYTES: usize = 16 * 1024 * 1024;

/// Bookkeeping cost of one chunk, counted towards the limit so that many tiny
/// chunks can't exceed it.
const CHUNK_OVERHEAD: usize = std::mem::size_of::<Chunk>();

fn chunk_cost(len: usize) -> usize {
    len + CHUNK_OVERHEAD
}

#[derive(Debug)]
pub struct DataLog {
    chunks: Vec<Chunk>,
    /// Sum of `chunk_cost` over `chunks`.
    stored: usize,
    limit: usize,
    /// Matching monotonic and wall-clock times, to turn chunk times into
    /// clock times for timestamps.
    epoch: (Instant, SystemTime),
    rx_total: u64,
    tx_total: u64,
    counter: ByteCounter,
}

impl Default for DataLog {
    fn default() -> Self {
        Self::with_limit(DEFAULT_LIMIT_BYTES)
    }
}

impl DataLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_limit(limit: usize) -> Self {
        Self {
            chunks: Vec::new(),
            stored: 0,
            limit: limit.max(1),
            epoch: (Instant::now(), SystemTime::now()),
            rx_total: 0,
            tx_total: 0,
            counter: ByteCounter::default(),
        }
    }

    /// Wall-clock time of a chunk's `at`.
    pub fn wall_time(&self, at: Instant) -> SystemTime {
        let (i0, s0) = self.epoch;
        match at.checked_duration_since(i0) {
            Some(d) => s0 + d,
            None => s0 - i0.duration_since(at),
        }
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Change the limit. Returns true if old data was dropped.
    pub fn set_limit(&mut self, limit: usize) -> bool {
        self.limit = limit.max(1);
        self.trim()
    }

    /// Add a chunk. Returns true if old data was dropped to stay within the
    /// limit; views built from the log must then be rebuilt.
    pub fn push(&mut self, dir: Direction, at: Instant, data: Vec<u8>) -> bool {
        if data.is_empty() {
            return false;
        }
        self.count(dir, &data);
        self.store(dir, at, data)
    }

    /// Add received data with "Clear at": if `clear_at` occurs in it, all
    /// received data up to and including its last occurrence is removed.
    /// Counters still count every byte. Returns `(trimmed, cleared)`; after
    /// either, views must be rebuilt.
    pub fn push_rx_clear_at(&mut self, at: Instant, data: Vec<u8>, clear_at: u8) -> (bool, bool) {
        let Some(pos) = data.iter().rposition(|&b| b == clear_at) else {
            return (self.push(Direction::Rx, at, data), false);
        };
        self.count(Direction::Rx, &data);
        self.clear(Direction::Rx);
        let tail = data[pos + 1..].to_vec();
        let trimmed = !tail.is_empty() && self.store(Direction::Rx, at, tail);
        (trimmed, true)
    }

    fn count(&mut self, dir: Direction, data: &[u8]) {
        match dir {
            Direction::Rx => {
                self.rx_total += data.len() as u64;
                let v = self.counter.value;
                self.counter.count += data.iter().filter(|&&b| b == v).count() as u64;
            }
            Direction::Tx => self.tx_total += data.len() as u64,
        }
    }

    fn store(&mut self, dir: Direction, at: Instant, data: Vec<u8>) -> bool {
        self.stored += chunk_cost(data.len());
        self.chunks.push(Chunk { dir, at, data });
        self.trim()
    }

    /// When over the limit, drop the oldest chunks down to 3/4 of it.
    fn trim(&mut self) -> bool {
        if self.stored <= self.limit {
            return false;
        }
        let target = self.limit / 4 * 3;
        let mut drop = 0;
        while self.stored > target && drop < self.chunks.len() {
            self.stored -= chunk_cost(self.chunks[drop].data.len());
            drop += 1;
        }
        self.chunks.drain(..drop);
        true
    }

    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }

    /// Chunks in one direction, or all when `dir` is `None` (combined view).
    pub fn chunks_for(&self, dir: Option<Direction>) -> impl Iterator<Item = &Chunk> {
        self.chunks
            .iter()
            .filter(move |c| dir.is_none_or(|d| c.dir == d))
    }

    /// Remove the data of one direction from the views ("Clear received" /
    /// "Clear transmitted"). Counters are kept; they have their own reset.
    pub fn clear(&mut self, dir: Direction) {
        self.chunks.retain(|c| c.dir != dir);
        self.stored = self.chunks.iter().map(|c| chunk_cost(c.data.len())).sum();
    }

    pub fn clear_all(&mut self) {
        self.chunks.clear();
        self.stored = 0;
    }

    pub fn rx_total(&self) -> u64 {
        self.rx_total
    }

    pub fn tx_total(&self) -> u64 {
        self.tx_total
    }

    pub fn reset_rx_total(&mut self) {
        self.rx_total = 0;
    }

    pub fn reset_tx_total(&mut self) {
        self.tx_total = 0;
    }

    pub fn counter(&self) -> ByteCounter {
        self.counter
    }

    /// Choose the byte to count. Resets the count.
    pub fn set_counted_byte(&mut self, value: u8) {
        self.counter = ByteCounter { value, count: 0 };
    }

    pub fn reset_counter(&mut self) {
        self.counter.count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_and_counter() {
        let mut log = DataLog::new();
        let now = Instant::now();
        log.push(Direction::Rx, now, b"a\0b\0".to_vec());
        log.push(Direction::Tx, now, b"xyz".to_vec());
        assert_eq!(log.rx_total(), 4);
        assert_eq!(log.tx_total(), 3);
        assert_eq!(log.counter().count, 2);

        log.clear(Direction::Rx);
        assert_eq!(log.chunks().len(), 1);
        assert_eq!(log.rx_total(), 4, "clearing the view keeps the counter");

        log.set_counted_byte(b'q');
        assert_eq!(
            log.counter(),
            ByteCounter {
                value: b'q',
                count: 0
            }
        );
    }

    #[test]
    fn limit_drops_oldest_quarter_at_once() {
        let per_chunk = chunk_cost(100);
        let mut log = DataLog::with_limit(per_chunk * 8);
        let now = Instant::now();
        for i in 0..8u8 {
            assert!(!log.push(Direction::Rx, now, vec![i; 100]));
        }
        // The 9th chunk goes over: drop down to 3/4 of the limit (6 chunks).
        assert!(log.push(Direction::Rx, now, vec![8; 100]));
        assert_eq!(log.chunks().len(), 6);
        assert_eq!(log.chunks()[0].data[0], 3, "oldest were dropped");
        assert_eq!(log.rx_total(), 900, "totals are unaffected");
        // Room again until the limit is reached.
        assert!(!log.push(Direction::Rx, now, vec![9; 100]));
    }

    #[test]
    fn clear_at_keeps_only_bytes_after_the_marker() {
        // docs/spec/views.md: "abc", then "deXfgh" with Clear at 'X' -> "fgh".
        let mut log = DataLog::new();
        let now = Instant::now();
        log.push(Direction::Tx, now, b"sent".to_vec());
        assert_eq!(
            log.push_rx_clear_at(now, b"abc".to_vec(), b'X'),
            (false, false)
        );
        assert_eq!(
            log.push_rx_clear_at(now, b"deXfgh".to_vec(), b'X'),
            (false, true)
        );
        let rx: Vec<u8> = log
            .chunks_for(Some(Direction::Rx))
            .flat_map(|c| c.data.clone())
            .collect();
        assert_eq!(rx, b"fgh");
        assert_eq!(log.rx_total(), 9, "counters count everything");
        assert_eq!(
            log.chunks_for(Some(Direction::Tx)).count(),
            1,
            "sent data stays"
        );
        // Marker as the last byte: nothing received is left.
        log.push_rx_clear_at(now, b"zX".to_vec(), b'X');
        assert_eq!(log.chunks_for(Some(Direction::Rx)).count(), 0);
    }

    #[test]
    fn direction_filter() {
        let mut log = DataLog::new();
        let now = Instant::now();
        log.push(Direction::Rx, now, b"r".to_vec());
        log.push(Direction::Tx, now, b"t".to_vec());
        assert_eq!(log.chunks_for(Some(Direction::Tx)).count(), 1);
        assert_eq!(log.chunks_for(None).count(), 2);
    }
}
