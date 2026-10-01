use std::sync::Arc;
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
    pub fn name(self) -> &'static str {
        match self {
            Urgency::Low => "low",
            Urgency::Normal => "normal",
            Urgency::Critical => "critical",
        }
    }

    pub fn from_byte(b: u8) -> Self {
        match b {
            0 => Urgency::Low,
            2 => Urgency::Critical,
            _ => Urgency::Normal,
        }
    }
}

/// Raw pixels from the `image-data` hint, converted to RGBA8.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageData {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}

impl ImageData {
    /// Converts the spec's `(iiibiiay)` layout: rows of `rowstride` bytes with
    /// 3 (RGB) or 4 (RGBA) channels of 8 bits. Returns `None` if inconsistent.
    pub fn from_spec(
        width: i32,
        height: i32,
        rowstride: i32,
        has_alpha: bool,
        bits_per_sample: i32,
        channels: i32,
        data: &[u8],
    ) -> Option<Self> {
        let (w, h, stride) = (
            usize::try_from(width).ok()?,
            usize::try_from(height).ok()?,
            usize::try_from(rowstride).ok()?,
        );
        let channels = usize::try_from(channels).ok()?;
        if bits_per_sample != 8 || channels != if has_alpha { 4 } else { 3 } || w == 0 || h == 0 {
            return None;
        }
        // the last row may be shorter than rowstride
        if stride < w * channels || data.len() < stride * (h - 1) + w * channels {
            return None;
        }
        let mut rgba = Vec::with_capacity(w * h * 4);
        for row in data.chunks(stride).take(h) {
            for px in row[..w * channels].chunks_exact(channels) {
                rgba.extend_from_slice(&px[..3]);
                rgba.push(if has_alpha { px[3] } else { 255 });
            }
        }
        Some(Self {
            width: w as u32,
            height: h as u32,
            rgba: rgba.into(),
        })
    }
}

/// The hints durst understands, parsed from the D-Bus `a{sv}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hints {
    pub urgency: Urgency,
    pub category: Option<String>,
    pub desktop_entry: Option<String>,
    pub image_path: Option<String>,
    pub image_data: Option<ImageData>,
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
    /// unix time in seconds
    pub received: u64,
}

impl Notification {
    /// How long the notification stays visible, `None` = until closed.
    pub fn timeout(&self, urgencies: &Urgencies) -> Option<Duration> {
        match self.expire_timeout {
            0 => None,
            ms if ms > 0 => Some(Duration::from_millis(ms as u64)),
            _ => urgencies.get(self.hints.urgency).timeout.0,
        }
    }
}

impl Notification {
    /// Whether `other` shows the same content, for duplicate stacking.
    pub fn same_content(&self, other: &Notification) -> bool {
        self.app_name == other.app_name
            && self.summary == other.summary
            && self.body == other.body
            && self.app_icon == other.app_icon
            && self.hints.urgency == other.hints.urgency
            && self.hints.value == other.hints.value
            && self.actions == other.actions
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
        received: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_follows_dbus_then_urgency() {
        let urgencies = crate::config::Config::default().urgency;
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
    fn image_data_conversion() {
        // 2x2 RGB with a padded rowstride of 8, last row unpadded
        let data = [1, 2, 3, 4, 5, 6, 0, 0, 7, 8, 9, 10, 11, 12];
        let img = ImageData::from_spec(2, 2, 8, false, 8, 3, &data).unwrap();
        assert_eq!(
            &img.rgba[..],
            &[1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]
        );
        let rgba = [9; 16];
        assert!(ImageData::from_spec(2, 2, 8, true, 8, 4, &rgba).is_some());
        assert!(ImageData::from_spec(2, 2, 8, true, 8, 3, &rgba).is_none());
        assert!(ImageData::from_spec(2, 3, 8, true, 8, 4, &rgba).is_none());
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
