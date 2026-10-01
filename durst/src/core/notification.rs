use std::time::Duration;

use crate::config::Urgencies;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Urgency {
    Low,
    #[default]
    Normal,
    Critical,
}

impl Urgency {
    pub fn from_byte(b: u8) -> Self {
        match b {
            0 => Urgency::Low,
            2 => Urgency::Critical,
            _ => Urgency::Normal,
        }
    }
}

/// The hints durst understands, parsed from the D-Bus `a{sv}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hints {
    pub urgency: Urgency,
    pub category: Option<String>,
    pub desktop_entry: Option<String>,
    pub image_path: Option<String>,
    pub transient: bool,
    pub resident: bool,
    /// progress in percent
    pub value: Option<i32>,
    /// x-dunst-stack-tag / x-canonical-private-synchronous
    pub stack_tag: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    /// (key, label) pairs
    pub actions: Vec<(String, String)>,
    pub hints: Hints,
    /// as sent over D-Bus: -1 = server default, 0 = never, >0 = milliseconds
    pub expire_timeout: i32,
}

impl Notification {
    /// How long the notification stays visible, `None` = until closed.
    pub fn timeout(&self, urgencies: &Urgencies) -> Option<Duration> {
        match self.expire_timeout {
            0 => None,
            ms if ms > 0 => Some(Duration::from_millis(ms as u64)),
            _ => {
                let config = match self.hints.urgency {
                    Urgency::Low => &urgencies.low,
                    Urgency::Normal => &urgencies.normal,
                    Urgency::Critical => &urgencies.critical,
                };
                config.timeout.0
            }
        }
    }
}

/// Turns the flat `[key, label, key, label, ...]` list from D-Bus into pairs;
/// a trailing key without label is dropped.
pub fn pair_actions<S: AsRef<str>>(flat: &[S]) -> Vec<(String, String)> {
    flat.as_chunks::<2>()
        .0
        .iter()
        .map(|[k, v]| (k.as_ref().to_owned(), v.as_ref().to_owned()))
        .collect()
}

#[cfg(test)]
pub fn test_notification(id: u32) -> Notification {
    Notification {
        id,
        app_name: "app".into(),
        app_icon: String::new(),
        summary: format!("summary {id}"),
        body: "body".into(),
        actions: vec![],
        hints: Hints::default(),
        expire_timeout: -1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_follows_dbus_then_urgency() {
        let urgencies = Urgencies::default();
        let mut n = test_notification(1);
        assert_eq!(n.timeout(&urgencies), Some(Duration::from_secs(10)));
        n.hints.urgency = Urgency::Low;
        assert_eq!(n.timeout(&urgencies), Some(Duration::from_secs(5)));
        n.hints.urgency = Urgency::Critical;
        assert_eq!(n.timeout(&urgencies), None);
        n.expire_timeout = 1500;
        assert_eq!(n.timeout(&urgencies), Some(Duration::from_millis(1500)));
        n.expire_timeout = 0;
        assert_eq!(n.timeout(&urgencies), None);
    }

    #[test]
    fn actions_are_paired() {
        assert_eq!(
            pair_actions(&["default", "Open", "no", "No", "dangling"]),
            vec![
                ("default".into(), "Open".into()),
                ("no".into(), "No".into())
            ]
        );
    }
}
