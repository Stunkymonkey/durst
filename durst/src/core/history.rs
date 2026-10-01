use std::collections::VecDeque;

use super::notification::Notification;

/// Closed notifications, newest first, at most `capacity` of them.
#[derive(Debug)]
pub struct History {
    entries: VecDeque<Notification>,
    capacity: usize,
}

impl History {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            capacity,
        }
    }

    /// Keeps `n` unless it is transient; the oldest entry falls out when full.
    pub fn push(&mut self, n: Notification) {
        if n.hints.transient || self.capacity == 0 {
            return;
        }
        // a replaced notification (same id) only appears once
        self.entries.retain(|e| e.id != n.id);
        self.entries.push_front(n);
        self.entries.truncate(self.capacity);
    }

    pub fn pop(&mut self) -> Option<Notification> {
        self.entries.pop_front()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Notification> {
        self.entries.iter()
    }

    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity;
        self.entries.truncate(capacity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::notification::test_notification;

    fn ids(h: &History) -> Vec<u32> {
        h.iter().map(|n| n.id).collect()
    }

    #[test]
    fn newest_first_and_bounded() {
        let mut h = History::new(2);
        for id in 1..=3 {
            h.push(test_notification(id));
        }
        assert_eq!(ids(&h), vec![3, 2]);
        assert_eq!(h.pop().map(|n| n.id), Some(3));
        assert_eq!(ids(&h), vec![2]);
        h.set_capacity(0);
        assert_eq!(h.len(), 0);
        h.push(test_notification(4));
        assert_eq!(h.len(), 0);
    }

    #[test]
    fn skips_transient_and_dedups_ids() {
        let mut h = History::new(5);
        let mut t = test_notification(1);
        t.hints.transient = true;
        h.push(t);
        assert_eq!(h.len(), 0);
        h.push(test_notification(2));
        h.push(test_notification(3));
        h.push(test_notification(2));
        assert_eq!(ids(&h), vec![2, 3]);
        h.clear();
        assert!(h.pop().is_none());
    }
}
