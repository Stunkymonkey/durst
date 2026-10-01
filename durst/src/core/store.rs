use std::time::{Duration, Instant};

use super::layout::{Margin, stack_margins};
use super::notification::Notification;
use crate::config::General;

#[derive(Debug)]
pub struct Entry {
    pub notification: Notification,
    /// surface height in pixels
    pub height: u32,
    pub expires_at: Option<Instant>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Upsert {
    Added,
    /// an entry with the same id was updated in place
    Replaced,
}

/// The displayed notifications, ordered from the anchored screen edge outwards.
#[derive(Debug, Default)]
pub struct Store {
    entries: Vec<Entry>,
}

impl Store {
    pub fn upsert(
        &mut self,
        notification: Notification,
        height: u32,
        timeout: Option<Duration>,
        now: Instant,
    ) -> Upsert {
        let entry = Entry {
            expires_at: timeout.map(|t| now + t),
            notification,
            height,
        };
        match self.position(entry.notification.id) {
            Some(i) => {
                self.entries[i] = entry;
                Upsert::Replaced
            }
            None => {
                self.entries.push(entry);
                Upsert::Added
            }
        }
    }

    pub fn remove(&mut self, id: u32) -> Option<Entry> {
        self.position(id).map(|i| self.entries.remove(i))
    }

    pub fn get(&self, id: u32) -> Option<&Entry> {
        self.entries.iter().find(|e| e.notification.id == id)
    }

    #[cfg(test)]
    pub fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    pub fn expired(&self, now: Instant) -> Vec<u32> {
        self.entries
            .iter()
            .filter(|e| e.expires_at.is_some_and(|t| t <= now))
            .map(|e| e.notification.id)
            .collect()
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.entries.iter().filter_map(|e| e.expires_at).min()
    }

    /// Margins of all entries, paired with their notification id.
    pub fn margins(&self, general: &General) -> Vec<(u32, Margin)> {
        let heights: Vec<u32> = self.entries.iter().map(|e| e.height).collect();
        self.entries
            .iter()
            .map(|e| e.notification.id)
            .zip(stack_margins(general, &heights))
            .collect()
    }

    fn position(&self, id: u32) -> Option<usize> {
        self.entries.iter().position(|e| e.notification.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::notification::test_notification;

    #[test]
    fn replace_keeps_position() {
        let now = Instant::now();
        let mut s = Store::default();
        s.upsert(test_notification(1), 10, None, now);
        s.upsert(test_notification(2), 10, None, now);
        let mut n = test_notification(1);
        n.summary = "new".into();
        assert_eq!(s.upsert(n, 20, None, now), Upsert::Replaced);
        let ids: Vec<u32> = s.iter().map(|e| e.notification.id).collect();
        assert_eq!(ids, vec![1, 2]);
        assert_eq!(s.get(1).unwrap().notification.summary, "new");
    }

    #[test]
    fn expiry() {
        let now = Instant::now();
        let mut s = Store::default();
        s.upsert(test_notification(1), 10, Some(Duration::from_secs(5)), now);
        s.upsert(test_notification(2), 10, Some(Duration::from_secs(2)), now);
        s.upsert(test_notification(3), 10, None, now);
        assert_eq!(s.next_deadline(), Some(now + Duration::from_secs(2)));
        assert!(s.expired(now + Duration::from_secs(1)).is_empty());
        assert_eq!(s.expired(now + Duration::from_secs(3)), vec![2]);
        assert_eq!(s.expired(now + Duration::from_secs(5)), vec![1, 2]);
    }

    #[test]
    fn removing_reflows_the_stack() {
        let now = Instant::now();
        let general = General {
            offset: [0, 0],
            gap: 10,
            ..General::default()
        };
        let mut s = Store::default();
        for (id, h) in [(1, 100), (2, 50), (3, 70)] {
            s.upsert(test_notification(id), h, None, now);
        }
        assert_eq!(s.margins(&general)[2], (3, (170, 0, 0, 0)));
        s.remove(1);
        assert_eq!(
            s.margins(&general),
            vec![(2, (0, 0, 0, 0)), (3, (60, 0, 0, 0))]
        );
    }
}
