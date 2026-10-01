//! TOML configuration, see `contrib/config.toml` for a documented example.

use iced::Color;
use serde::{Deserialize, Deserializer, de};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub general: General,
    pub style: Style,
    pub urgency: Urgencies,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    pub anchor: Anchor,
    /// distance from the anchored screen edges: [x, y]
    pub offset: [i32; 2],
    /// space between two notifications
    pub gap: u32,
    pub width: u32,
}

impl Default for General {
    fn default() -> Self {
        Self {
            anchor: Anchor::TopRight,
            offset: [20, 20],
            gap: 8,
            width: 380,
        }
    }
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    pub fn is_top(self) -> bool {
        matches!(self, Anchor::TopLeft | Anchor::Top | Anchor::TopRight)
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Style {
    pub font_size: f32,
    /// inner space between border and content
    pub padding: u32,
    /// space between icon and text, and between summary and body
    pub spacing: u32,
    pub icon_size: u32,
    #[serde(deserialize_with = "color")]
    pub background: Color,
    #[serde(deserialize_with = "color")]
    pub foreground: Color,
    pub border: Border,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            padding: 12,
            spacing: 8,
            icon_size: 48,
            background: Color::from_rgba8(0x1e, 0x1e, 0x2e, 0.9),
            foreground: Color::from_rgb8(0xcd, 0xd6, 0xf4),
            border: Border::default(),
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Border {
    pub width: u32,
    #[serde(deserialize_with = "color")]
    pub color: Color,
    pub radius: f32,
}

impl Default for Border {
    fn default() -> Self {
        Self {
            width: 2,
            color: Color::from_rgb8(0x89, 0xb4, 0xfa),
            radius: 10.0,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Urgencies {
    pub low: UrgencyConfig,
    pub normal: UrgencyConfig,
    pub critical: UrgencyConfig,
}

impl Default for Urgencies {
    fn default() -> Self {
        let secs = |s| UrgencyConfig {
            timeout: Timeout(Some(Duration::from_secs(s))),
        };
        Self {
            low: secs(5),
            normal: secs(10),
            critical: UrgencyConfig {
                timeout: Timeout(None),
            },
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct UrgencyConfig {
    pub timeout: Timeout,
}

/// `None` means the notification never expires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeout(pub Option<Duration>);

impl<'de> Deserialize<'de> for Timeout {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Secs(u64),
            Text(String),
        }
        let duration = match Raw::deserialize(d)? {
            Raw::Secs(s) => Duration::from_secs(s),
            Raw::Text(t) => parse_duration(&t).map_err(de::Error::custom)?,
        };
        Ok(Timeout((!duration.is_zero()).then_some(duration)))
    }
}

/// Parses "500ms", "5s", "2m" or a plain number of seconds.
fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let n: u64 = num.parse().map_err(|_| format!("invalid duration {s:?}"))?;
    match unit.trim() {
        "ms" => Ok(Duration::from_millis(n)),
        "" | "s" => Ok(Duration::from_secs(n)),
        "m" => Ok(Duration::from_secs(n * 60)),
        _ => Err(format!("invalid duration unit in {s:?}, use ms, s or m")),
    }
}

/// Parses "#rrggbb" or "#rrggbbaa".
fn parse_color(s: &str) -> Result<Color, String> {
    let hex = s
        .strip_prefix('#')
        .filter(|h| (h.len() == 6 || h.len() == 8) && h.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| format!("invalid color {s:?}, expected #rrggbb or #rrggbbaa"))?;
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
    let alpha = if hex.len() == 8 {
        byte(6) as f32 / 255.0
    } else {
        1.0
    };
    Ok(Color::from_rgba8(byte(0), byte(2), byte(4), alpha))
}

fn color<'de, D: Deserializer<'de>>(d: D) -> Result<Color, D::Error> {
    parse_color(&String::deserialize(d)?).map_err(de::Error::custom)
}

pub fn default_path() -> PathBuf {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_default();
    config_home.join("durst/config.toml")
}

/// Loads the config. A missing file at the default location yields the
/// defaults; every other problem is an error.
pub fn load(path: &Path, explicit: bool) -> Result<Config, String> {
    match std::fs::read_to_string(path) {
        Ok(s) => parse(&s).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !explicit => {
            log::info!("no config at {}, using defaults", path.display());
            Ok(Config::default())
        }
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

pub fn parse(s: &str) -> Result<Config, toml::de::Error> {
    toml::from_str(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_is_default() {
        let c = parse("").unwrap();
        assert_eq!(c.general.anchor, Anchor::TopRight);
        assert_eq!(c.urgency.critical.timeout, Timeout(None));
    }

    #[test]
    fn example_config_parses() {
        parse(include_str!("../../contrib/config.toml")).unwrap();
    }

    #[test]
    fn unknown_key_is_reported_with_location() {
        let err = parse("[general]\nwidht = 3\n").unwrap_err().to_string();
        assert!(err.contains("widht"), "{err}");
        assert!(err.contains("line 2"), "{err}");
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("500ms"), Ok(Duration::from_millis(500)));
        assert_eq!(parse_duration("5s"), Ok(Duration::from_secs(5)));
        assert_eq!(parse_duration("2m"), Ok(Duration::from_secs(120)));
        assert_eq!(parse_duration("7"), Ok(Duration::from_secs(7)));
        assert!(parse_duration("5h").is_err());
        let c = parse("[urgency.low]\ntimeout = 0\n[urgency.normal]\ntimeout = \"0\"").unwrap();
        assert_eq!(c.urgency.low.timeout, Timeout(None));
        assert_eq!(c.urgency.normal.timeout, Timeout(None));
    }

    #[test]
    fn colors() {
        assert_eq!(parse_color("#ff0000"), Ok(Color::from_rgb8(255, 0, 0)));
        assert_eq!(parse_color("#00ff0080").unwrap().a, 128.0 / 255.0);
        assert!(parse_color("ff0000").is_err());
        assert!(parse_color("#ff00").is_err());
    }
}
