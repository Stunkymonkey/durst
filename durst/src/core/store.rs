use std::cmp::Reverse;
use std::time::{Duration, Instant};

use super::layout::{Margin, stack_margins};
use super::notification::Notification;
use crate::config::General;

/// Remaining display time. It only runs while the notification is visible and
/// not hovered, so waiting or hovered notifications don't expire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Timer {
    /// `None` = never expires
    left: Option<Duration>,
    running_since: Option<Instant>,
}

impl Timer {
    fn new(timeout: Option<Duration>) -> Self {
        Self {
            left: timeout,
            running_since: None,
        }
    }

    fn resume(&mut self, now: Instant) {
        if self.left.is_some() && self.running_since.is_none() {
            self.running_since = Some(now);
        }
    }

    fn pause(&mut self, now: Instant) {
        if let (Some(left), Some(since)) = (self.left, self.running_since.take()) {
            self.left = Some(left.saturating_sub(now - since));
        }
    }

    fn deadline(&self) -> Option<Instant> {
        Some(self.running_since? + self.left?)
    }
}

#[derive(Debug)]
pub struct Entry {
    pub notification: Notification,
    /// surface height in pixels
    pub height: u32,
    /// how many identical notifications this entry stands for
    pub count: u32,
    pub hovered: bool,
    /// arrival order, kept when the entry is replaced
    seq: u64,
    timer: Timer,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Insert {
    Added,
    /// an entry with the same id was updated in place
    Replaced,
    /// the notification took the place of the entry with this id: a duplicate
    /// (its counter is increased) or one with the same stack tag
    Superseded(u32),
}

/// All notifications: the first `max_visible` are shown, the rest wait.
#[derive(Debug, Default)]
pub struct Store {
    /// sorted, the entry next to the anchored screen edge first
    entries: Vec<Entry>,
    next_seq: u64,
}

impl Store {
    pub fn insert(
        &mut self,
        notification: Notification,
        height: u32,
        timeout: Option<Duration>,
        general: &General,
    ) -> Insert {
        let mut entry = Entry {
            notification,
            height,
            count: 1,
            hovered: false,
            seq: self.next_seq,
            timer: Timer::new(timeout),
        };
        let n = &entry.notification;
        let (result, i) = if let Some(i) = self.position(n.id) {
            (Insert::Replaced, i)
        } else if let Some(i) = n.hints.stack_tag.as_ref().and_then(|tag| {
            self.entries
                .iter()
                .position(|e| e.notification.hints.stack_tag.as_ref() == Some(tag))
        }) {
            (Insert::Superseded(self.entries[i].notification.id), i)
        } else if let Some(i) = general
            .stack_duplicates
            .then(|| {
                self.entries
                    .iter()
                    .position(|e| e.notification.same_content(n))
            })
            .flatten()
        {
            entry.count = self.entries[i].count + 1;
            (Insert::Superseded(self.entries[i].notification.id), i)
        } else {
            self.next_seq += 1;
            self.entries.push(entry);
            self.sort(general);
            return Insert::Added;
        };
        let old = &self.entries[i];
        entry.seq = old.seq;
        entry.hovered = old.hovered;
        if result == Insert::Replaced {
            entry.count = old.count;
        }
        self.entries[i] = entry;
        self.sort(general);
        result
    }

    fn sort(&mut self, general: &General) {
        self.entries.sort_by_key(|e| {
            let urgency = general
                .sort_by_urgency
                .then_some(Reverse(e.notification.hints.urgency));
            let seq = if general.newest_first {
                Reverse(e.seq)
            } else {
                Reverse(u64::MAX - e.seq)
            };
            (urgency, seq)
        });
    }

    pub fn remove(&mut self, id: u32) -> Option<Entry> {
        self.position(id).map(|i| self.entries.remove(i))
    }

    pub fn get(&self, id: u32) -> Option<&Entry> {
        self.entries.iter().find(|e| e.notification.id == id)
    }

    pub fn visible(&self, general: &General) -> &[Entry] {
        let n = match general.max_visible {
            0 => self.entries.len(),
            max => max.min(self.entries.len()),
        };
        &self.entries[..n]
    }

    pub fn waiting(&self, general: &General) -> usize {
        self.entries.len() - self.visible(general).len()
    }

    pub fn set_height(&mut self, id: u32, height: u32) {
        if let Some(i) = self.position(id) {
            self.entries[i].height = height;
        }
    }

    pub fn set_hovered(&mut self, id: u32, hovered: bool) {
        if let Some(i) = self.position(id) {
            self.entries[i].hovered = hovered;
        }
    }

    /// Runs the timers of visible, unhovered entries and pauses all others.
    pub fn update_timers(&mut self, general: &General, now: Instant) {
        let visible = self.visible(general).len();
        for (i, e) in self.entries.iter_mut().enumerate() {
            if i < visible && !e.hovered {
                e.timer.resume(now);
            } else {
                e.timer.pause(now);
            }
        }
    }

    pub fn expired(&self, now: Instant) -> Vec<u32> {
        self.entries
            .iter()
            .filter(|e| e.timer.deadline().is_some_and(|t| t <= now))
            .map(|e| e.notification.id)
            .collect()
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.entries.iter().filter_map(|e| e.timer.deadline()).min()
    }

    /// Margins of the visible entries, plus one for an extra surface of
    /// `extra_height` after them (the "+N more" indicator).
    pub fn margins(&self, general: &General, extra_height: Option<u32>) -> Vec<Margin> {
        let heights: Vec<u32> = self
            .visible(general)
            .iter()
            .map(|e| e.height)
            .chain(extra_height)
            .collect();
        stack_margins(general, &heights)
    }

    fn position(&self, id: u32) -> Option<usize> {
        self.entries.iter().position(|e| e.notification.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::notification::{Urgency, test_notification};

    const SEC: Duration = Duration::from_secs(1);

    fn general() -> General {
        General {
            offset: [0, 0],
            gap: 10,
            max_visible: 0,
            ..General::default()
        }
    }

    fn ids(entries: &[Entry]) -> Vec<u32> {
        entries.iter().map(|e| e.notification.id).collect()
    }

    fn insert(s: &mut Store, n: Notification, g: &General) -> Insert {
        s.insert(n, 10, None, g)
    }

    #[test]
    fn replace_keeps_position_and_count() {
        let g = general();
        let mut s = Store::default();
        insert(&mut s, test_notification(1), &g);
        insert(&mut s, test_notification(2), &g);
        let mut n = test_notification(1);
        n.summary = "new".into();
        assert_eq!(insert(&mut s, n, &g), Insert::Replaced);
        assert_eq!(ids(s.visible(&g)), vec![1, 2]);
        assert_eq!(s.get(1).unwrap().notification.summary, "new");
    }

    #[test]
    fn sorting() {
        let mut g = general();
        let mut s = Store::default();
        let critical = |id| {
            let mut n = test_notification(id);
            n.hints.urgency = Urgency::Critical;
            n
        };
        insert(&mut s, test_notification(1), &g);
        insert(&mut s, critical(2), &g);
        insert(&mut s, test_notification(3), &g);
        assert_eq!(ids(s.visible(&g)), vec![2, 1, 3]);
        g.newest_first = true;
        insert(&mut s, test_notification(4), &g);
        assert_eq!(ids(s.visible(&g)), vec![2, 4, 3, 1]);
        g.sort_by_urgency = false;
        insert(&mut s, test_notification(5), &g);
        assert_eq!(ids(s.visible(&g)), vec![5, 4, 3, 2, 1]);
    }

    #[test]
    fn duplicates_are_counted() {
        let mut g = general();
        let mut s = Store::default();
        let dup = |id| {
            let mut n = test_notification(id);
            n.summary = "same".into();
            n
        };
        insert(&mut s, dup(1), &g);
        insert(&mut s, test_notification(2), &g);
        assert_eq!(insert(&mut s, dup(3), &g), Insert::Superseded(1));
        assert_eq!(insert(&mut s, dup(4), &g), Insert::Superseded(3));
        assert_eq!(ids(s.visible(&g)), vec![4, 2]);
        assert_eq!(s.get(4).unwrap().count, 3);
        g.stack_duplicates = false;
        assert_eq!(insert(&mut s, dup(5), &g), Insert::Added);
    }

    #[test]
    fn stack_tag_replaces_without_counting() {
        let g = general();
        let mut s = Store::default();
        let tagged = |id, body: &str| {
            let mut n = test_notification(id);
            n.body = body.into();
            n.hints.stack_tag = Some("volume".into());
            n
        };
        insert(&mut s, tagged(1, "10%"), &g);
        assert_eq!(insert(&mut s, tagged(2, "20%"), &g), Insert::Superseded(1));
        assert_eq!(ids(s.visible(&g)), vec![2]);
        assert_eq!(s.get(2).unwrap().count, 1);
    }

    #[test]
    fn max_visible_and_waiting() {
        let g = General {
            max_visible: 2,
            ..general()
        };
        let mut s = Store::default();
        for id in 1..=4 {
            insert(&mut s, test_notification(id), &g);
        }
        assert_eq!(ids(s.visible(&g)), vec![1, 2]);
        assert_eq!(s.waiting(&g), 2);
        s.remove(1);
        assert_eq!(ids(s.visible(&g)), vec![2, 3]);
    }

    #[test]
    fn timers_run_only_while_visible_and_unhovered() {
        let g = General {
            max_visible: 1,
            ..general()
        };
        let t0 = Instant::now();
        let mut s = Store::default();
        s.insert(test_notification(1), 10, Some(5 * SEC), &g);
        s.insert(test_notification(2), 10, Some(5 * SEC), &g);
        assert_eq!(s.next_deadline(), None, "nothing runs before update_timers");
        s.update_timers(&g, t0);
        assert_eq!(
            s.next_deadline(),
            Some(t0 + 5 * SEC),
            "only the visible one"
        );

        // hovered for 10s at t=1: paused with 4s left
        s.set_hovered(1, true);
        s.update_timers(&g, t0 + SEC);
        assert!(s.expired(t0 + 10 * SEC).is_empty());
        s.set_hovered(1, false);
        s.update_timers(&g, t0 + 11 * SEC);
        assert_eq!(s.next_deadline(), Some(t0 + 15 * SEC));
        assert_eq!(s.expired(t0 + 15 * SEC), vec![1]);

        // the waiting one starts its full timeout once it becomes visible
        s.remove(1);
        s.update_timers(&g, t0 + 15 * SEC);
        assert_eq!(s.next_deadline(), Some(t0 + 20 * SEC));
    }

    #[test]
    fn margins_with_indicator() {
        let g = General {
            max_visible: 2,
            ..general()
        };
        let mut s = Store::default();
        for (id, h) in [(1, 100), (2, 50), (3, 70)] {
            s.insert(test_notification(id), h, None, &g);
        }
        assert_eq!(
            s.margins(&g, Some(20)),
            vec![(0, 0, 0, 0), (110, 0, 0, 0), (170, 0, 0, 0)]
        );
        s.remove(1);
        assert_eq!(s.margins(&g, None), vec![(0, 0, 0, 0), (60, 0, 0, 0)]);
    }
}
