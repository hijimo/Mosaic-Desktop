//! A capped buffer that preserves a stable prefix ("head") and suffix ("tail"),
//! dropping the middle once it exceeds the configured maximum.

use std::collections::VecDeque;

use super::OUTPUT_MAX_BYTES;

/// Head-tail buffer: 50% head, 50% tail, drops middle on overflow.
#[derive(Debug)]
pub struct HeadTailBuffer {
    head_budget: usize,
    tail_budget: usize,
    head: VecDeque<Vec<u8>>,
    tail: VecDeque<Vec<u8>>,
    head_bytes: usize,
    tail_bytes: usize,
    omitted_bytes: usize,
}

impl Default for HeadTailBuffer {
    fn default() -> Self {
        Self::new(OUTPUT_MAX_BYTES)
    }
}

impl HeadTailBuffer {
    pub fn new(max_bytes: usize) -> Self {
        let head_budget = max_bytes / 2;
        let tail_budget = max_bytes.saturating_sub(head_budget);
        Self {
            head_budget,
            tail_budget,
            head: VecDeque::new(),
            tail: VecDeque::new(),
            head_bytes: 0,
            tail_bytes: 0,
            omitted_bytes: 0,
        }
    }

    pub fn retained_bytes(&self) -> usize {
        self.head_bytes.saturating_add(self.tail_bytes)
    }

    pub fn omitted_bytes(&self) -> usize {
        self.omitted_bytes
    }

    pub fn push_chunk(&mut self, chunk: Vec<u8>) {
        if self.head_budget == 0 && self.tail_budget == 0 {
            self.omitted_bytes = self.omitted_bytes.saturating_add(chunk.len());
            return;
        }

        if self.head_bytes < self.head_budget {
            let remaining = self.head_budget.saturating_sub(self.head_bytes);
            if chunk.len() <= remaining {
                self.head_bytes += chunk.len();
                self.head.push_back(chunk);
                return;
            }
            let (head_part, tail_part) = chunk.split_at(remaining);
            if !head_part.is_empty() {
                self.head_bytes += head_part.len();
                self.head.push_back(head_part.to_vec());
            }
            self.push_to_tail(tail_part.to_vec());
            return;
        }

        self.push_to_tail(chunk);
    }

    pub fn snapshot_chunks(&self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        out.extend(self.head.iter().cloned());
        out.extend(self.tail.iter().cloned());
        out
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.retained_bytes());
        for c in &self.head {
            out.extend_from_slice(c);
        }
        for c in &self.tail {
            out.extend_from_slice(c);
        }
        out
    }

    pub fn drain_chunks(&mut self) -> Vec<Vec<u8>> {
        let mut out: Vec<Vec<u8>> = self.head.drain(..).collect();
        out.extend(self.tail.drain(..));
        self.head_bytes = 0;
        self.tail_bytes = 0;
        self.omitted_bytes = 0;
        out
    }

    fn push_to_tail(&mut self, chunk: Vec<u8>) {
        if self.tail_budget == 0 {
            self.omitted_bytes = self.omitted_bytes.saturating_add(chunk.len());
            return;
        }

        if chunk.len() >= self.tail_budget {
            let start = chunk.len().saturating_sub(self.tail_budget);
            let kept = chunk[start..].to_vec();
            let dropped = chunk.len() - kept.len();
            self.omitted_bytes = self
                .omitted_bytes
                .saturating_add(self.tail_bytes)
                .saturating_add(dropped);
            self.tail.clear();
            self.tail_bytes = kept.len();
            self.tail.push_back(kept);
            return;
        }

        self.tail_bytes += chunk.len();
        self.tail.push_back(chunk);

        // Trim oldest tail chunks to stay within budget.
        let mut excess = self.tail_bytes.saturating_sub(self.tail_budget);
        while excess > 0 {
            match self.tail.front_mut() {
                Some(front) if excess >= front.len() => {
                    let len = front.len();
                    self.tail_bytes -= len;
                    self.omitted_bytes += len;
                    excess -= len;
                    self.tail.pop_front();
                }
                Some(front) => {
                    front.drain(..excess);
                    self.tail_bytes -= excess;
                    self.omitted_bytes += excess;
                    break;
                }
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_prefix_and_suffix_when_over_budget() {
        let mut buf = HeadTailBuffer::new(10);
        buf.push_chunk(b"0123456789".to_vec());
        assert_eq!(buf.omitted_bytes(), 0);

        buf.push_chunk(b"ab".to_vec());
        assert!(buf.omitted_bytes() > 0);

        let rendered = String::from_utf8_lossy(&buf.to_bytes()).to_string();
        assert!(rendered.starts_with("01234"));
        assert!(rendered.ends_with("ab"));
    }

    #[test]
    fn max_bytes_zero_drops_everything() {
        let mut buf = HeadTailBuffer::new(0);
        buf.push_chunk(b"abc".to_vec());
        assert_eq!(buf.retained_bytes(), 0);
        assert_eq!(buf.omitted_bytes(), 3);
    }

    #[test]
    fn draining_resets_state() {
        let mut buf = HeadTailBuffer::new(10);
        buf.push_chunk(b"0123456789".to_vec());
        buf.push_chunk(b"ab".to_vec());
        let drained = buf.drain_chunks();
        assert!(!drained.is_empty());
        assert_eq!(buf.retained_bytes(), 0);
        assert_eq!(buf.omitted_bytes(), 0);
    }

    #[test]
    fn fills_head_then_tail_across_multiple_chunks() {
        let mut buf = HeadTailBuffer::new(10);
        buf.push_chunk(b"01".to_vec());
        buf.push_chunk(b"234".to_vec());
        assert_eq!(buf.to_bytes(), b"01234");

        buf.push_chunk(b"567".to_vec());
        buf.push_chunk(b"89".to_vec());
        assert_eq!(buf.to_bytes(), b"0123456789");
        assert_eq!(buf.omitted_bytes(), 0);

        buf.push_chunk(b"a".to_vec());
        assert_eq!(buf.to_bytes(), b"012346789a");
        assert_eq!(buf.omitted_bytes(), 1);
    }

    #[test]
    fn chunk_larger_than_tail_budget_keeps_only_tail_end() {
        let mut buf = HeadTailBuffer::new(10);
        buf.push_chunk(b"0123456789".to_vec());
        buf.push_chunk(b"ABCDEFGHIJK".to_vec());

        let out = String::from_utf8_lossy(&buf.to_bytes()).to_string();
        assert!(out.starts_with("01234"));
        assert!(out.ends_with("GHIJK"));
        assert!(buf.omitted_bytes() > 0);
    }
}
