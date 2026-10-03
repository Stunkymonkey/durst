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

/// `image-data` hints larger than this per side are ignored.
pub const MAX_IMAGE_SIDE: u32 = 4096;
/// Kept at most this large per side: drawn at icon size anyway, and kept
/// with the notification and in the history.
pub const KEPT_IMAGE_SIDE: u32 = 256;

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
        if w.max(h) > MAX_IMAGE_SIDE as usize {
            log::warn!("ignoring image-data hint of {w}x{h}, larger than {MAX_IMAGE_SIDE}");
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
        Some(Self::shrunk(w as u32, h as u32, rgba))
    }

    /// Scales images larger than [`KEPT_IMAGE_SIDE`] down to it.
    fn shrunk(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        let largest = width.max(height);
        if largest <= KEPT_IMAGE_SIDE {
            return Self {
                width,
                height,
                rgba: rgba.into(),
            };
        }
        let scale = |n: u32| ((n as u64 * KEPT_IMAGE_SIDE as u64 / largest as u64) as u32).max(1);
        let (w, h) = (scale(width), scale(height));
        let img =
            image::RgbaImage::from_raw(width, height, rgba).expect("width * height * 4 bytes");
        let img = image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle);
        Self {
            width: w,
            height: h,
            rgba: img.into_raw().into(),
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
    pub image_data: Option<ImageData>,
    pub transient: bool,
    pub resident: bool,
    /// progress in percent
    pub value: Option<i32>,
    /// x-dunst-stack-tag / x-canonical-private-synchronous
    pub stack_tag: Option<String>,
    pub sound_file: Option<String>,
    pub sound_name: Option<String>,
    pub suppress_sound: bool,
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
    /// `ignore_dbus` uses the urgency's timeout even if the sender sets one.
    pub fn timeout(&self, urgencies: &Urgencies, ignore_dbus: bool) -> Option<Duration> {
        match self.expire_timeout {
            _ if ignore_dbus => urgencies.get(self.hints.urgency).timeout.0,
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
        assert_eq!(n.timeout(&urgencies, false), Some(Duration::from_secs(10)));
        n.hints.urgency = Urgency::Low;
        assert_eq!(n.timeout(&urgencies, false), Some(Duration::from_secs(5)));
        n.hints.urgency = Urgency::Critical;
        assert_eq!(n.timeout(&urgencies, false), None);
        n.expire_timeout = 1500;
        assert_eq!(
            n.timeout(&urgencies, false),
            Some(Duration::from_millis(1500))
        );
        n.expire_timeout = 0;
        assert_eq!(n.timeout(&urgencies, false), None);
        n.hints.urgency = Urgency::Low;
        assert_eq!(n.timeout(&urgencies, true), Some(Duration::from_secs(5)));
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
    fn large_image_data_is_shrunk_or_ignored() {
        let data = vec![7; 600 * 300 * 3];
        let img = ImageData::from_spec(600, 300, 1800, false, 8, 3, &data).unwrap();
        assert_eq!((img.width, img.height), (256, 128));
        assert_eq!(img.rgba.len(), 256 * 128 * 4);
        assert_eq!(&img.rgba[..4], &[7, 7, 7, 255]);
        let wide = vec![0; 5000 * 3];
        assert!(ImageData::from_spec(5000, 1, 15000, false, 8, 3, &wide).is_none());
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
