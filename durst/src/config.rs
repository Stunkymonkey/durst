//! TOML configuration, see `contrib/config.toml` for a documented example.

use iced::Color;
use serde::{Deserialize, Deserializer, de};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::core::notification::Urgency;
use crate::core::rules::Rule;

#[derive(Debug, Clone)]
pub struct Config {
    pub general: General,
    pub history: HistoryConfig,
    pub mouse: Mouse,
    pub urgency: Urgencies,
    pub sound: Sound,
    pub osd: Osd,
    pub rules: Vec<Rule>,
    /// the base style with each urgency's overrides applied, see [`Config::style`]
    styles: [Style; 3],
    /// the same as tables, for merging rule overrides
    style_tables: [toml::Table; 3],
}

impl Config {
    pub fn style(&self, urgency: Urgency) -> &Style {
        &self.styles[urgency as usize]
    }

    /// The urgency's style with `overrides` (from rules) merged over it.
    pub fn style_with(&self, urgency: Urgency, overrides: &toml::Table) -> Result<Style, String> {
        if overrides.is_empty() {
            return Ok(self.style(urgency).clone());
        }
        let mut table = self.style_tables[urgency as usize].clone();
        merge(&mut table, overrides);
        toml::Value::Table(table)
            .try_into::<Style>()
            .map_err(|e| e.to_string())
    }
}

impl Default for Config {
    fn default() -> Self {
        parse("").expect("default config")
    }
}

/// The config as written; styles are kept as tables so per-urgency overrides
/// can be merged over the base style before they are deserialized.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawConfig {
    general: General,
    history: HistoryConfig,
    mouse: Mouse,
    style: toml::Table,
    urgency: RawUrgencies,
    sound: Sound,
    osd: Osd,
    rule: Vec<Rule>,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawUrgencies {
    low: RawUrgency,
    normal: RawUrgency,
    critical: RawUrgency,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawUrgency {
    timeout: Option<Timeout>,
    style: toml::Table,
}

/// Overrides applied between the user's base style and their per-urgency
/// style, so critical notifications stand out unless configured otherwise.
const DEFAULT_URGENCY_STYLES: [&str; 3] = ["", "", "border.color = \"#f38ba8\""];

#[derive(Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    pub anchor: Anchor,
    /// distance from the anchored screen edges: [x, y]
    pub offset: [i32; 2],
    /// space between two notifications
    pub gap: u32,
    pub width: u32,
    /// how many notifications are shown at once, 0 = unlimited; the rest wait
    pub max_visible: usize,
    /// critical before normal before low, then by arrival
    pub sort_by_urgency: bool,
    /// put the newest notification next to the anchored edge
    pub newest_first: bool,
    /// merge identical notifications into one with a counter
    pub stack_duplicates: bool,
    pub output: Output,
    /// command used to open links, the URL is appended
    pub browser: Vec<String>,
    /// icon theme for icon names; default: GTK's `gtk-icon-theme-name`
    pub icon_theme: Option<String>,
    /// use the urgency's timeout even if the sender asks for another one
    pub ignore_dbus_timeout: bool,
    /// timeouts pause after this long without input; 0 = never
    pub idle_threshold: Timeout,
}

impl Default for General {
    fn default() -> Self {
        Self {
            anchor: Anchor::TopRight,
            offset: [20, 20],
            gap: 8,
            width: 380,
            max_visible: 5,
            sort_by_urgency: true,
            newest_first: false,
            stack_duplicates: true,
            output: Output::Focused,
            browser: vec!["xdg-open".into()],
            icon_theme: None,
            ignore_dbus_timeout: false,
            idle_threshold: Timeout(Some(Duration::from_secs(120))),
        }
    }
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Osd {
    pub volume: VolumeOsd,
    pub media: MediaOsd,
}

/// The on-screen display for the media player: cover, title, artist and
/// buttons; it uses the normal urgency's style.
#[derive(Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct MediaOsd {
    pub enabled: bool,
    pub anchor: Anchor,
    /// distance from the anchored screen edges: [x, y]
    pub offset: [i32; 2],
    pub width: u32,
    pub cover_size: u32,
    /// hidden after this long, unless the pointer is over it
    pub timeout: Timeout,
    /// show it when the playing track changes
    pub show_on_track_change: bool,
}

impl Default for MediaOsd {
    fn default() -> Self {
        Self {
            enabled: true,
            anchor: Anchor::Top,
            offset: [0, 20],
            width: 420,
            cover_size: 72,
            timeout: Timeout(Some(Duration::from_secs(4))),
            show_on_track_change: true,
        }
    }
}

/// The on-screen display for speaker and microphone volume; it uses the
/// normal urgency's style, the slider the progress colors.
#[derive(Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct VolumeOsd {
    pub enabled: bool,
    pub anchor: Anchor,
    /// distance from the anchored screen edges: [x, y]
    pub offset: [i32; 2],
    pub width: u32,
    /// hidden after this long, unless the pointer is over it
    pub timeout: Timeout,
    /// percent per scroll step and for `durstctl volume up/down`
    pub step: u32,
    /// the slider's maximum in percent; above 100 amplifies
    pub max_volume: u32,
    /// also show it when another program changes the volume
    pub show_on_external_change: bool,
}

impl Default for VolumeOsd {
    fn default() -> Self {
        Self {
            enabled: true,
            anchor: Anchor::Bottom,
            offset: [0, 80],
            width: 360,
            timeout: Timeout(Some(Duration::from_secs(2))),
            step: 5,
            max_volume: 100,
            show_on_external_change: true,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Sound {
    /// plays a sound file, the path is appended
    pub command: Vec<String>,
    /// sound theme for sound names (freedesktop sound theme spec)
    pub theme: String,
    /// play the `sound-file` / `sound-name` hints sent by applications
    pub play_hints: bool,
}

impl Default for Sound {
    fn default() -> Self {
        Self {
            command: vec!["pw-play".into()],
            theme: "freedesktop".into(),
            play_hints: true,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct HistoryConfig {
    /// how many closed notifications are kept, 0 = no history
    pub length: usize,
    /// notifications shown again by `durstctl history pop` don't expire
    pub sticky: bool,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            length: 20,
            sticky: true,
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

/// `"focused"`, `"all"` or `"name:<output>"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// the output of the focused window when the first notification
    /// appears (the compositor decides if there is none); the stack stays
    /// there until it is empty
    Focused,
    /// every notification on every output
    All,
    Name(String),
}

impl<'de> Deserialize<'de> for Output {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "focused" => Ok(Output::Focused),
            "all" => Ok(Output::All),
            _ => match s.strip_prefix("name:") {
                Some(name) if !name.is_empty() => Ok(Output::Name(name.to_owned())),
                _ => Err(de::Error::custom(format!(
                    "invalid output {s:?}, expected \"focused\", \"all\" or \"name:<output>\""
                ))),
            },
        }
    }
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MouseAction {
    None,
    CloseCurrent,
    CloseAll,
    /// invoke the "default" action, or the only action if there is just one
    DoAction,
    /// open the first link of the body
    OpenUrl,
    /// show the last closed notification again, like `durstctl history pop`
    HistoryPop,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Mouse {
    pub left: Vec<MouseAction>,
    pub middle: Vec<MouseAction>,
    pub right: Vec<MouseAction>,
    pub scroll_up: Vec<MouseAction>,
    pub scroll_down: Vec<MouseAction>,
}

impl Default for Mouse {
    fn default() -> Self {
        use MouseAction::*;
        Self {
            left: vec![CloseCurrent],
            middle: vec![DoAction, CloseCurrent],
            right: vec![CloseAll],
            scroll_up: vec![],
            scroll_down: vec![],
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Style {
    /// font family name; `None`: the system's sans-serif
    #[serde(deserialize_with = "font_family")]
    pub font: Option<&'static str>,
    pub font_size: f32,
    /// body lines shown at most, the rest is cut off with "…"; 0 = all
    pub max_lines: u32,
    /// the app name, small, above the summary
    pub show_app_name: bool,
    /// inner space between border and content
    pub padding: u32,
    /// space between icon and text, and between the parts of the text
    pub spacing: u32,
    /// size of SVG icons, and the size asked for from the icon theme
    pub icon_size: u32,
    /// raster images (and theme icons) keep their own size within
    /// `min_icon_size..=max_icon_size`; both default to `icon_size`
    pub min_icon_size: Option<u32>,
    pub max_icon_size: Option<u32>,
    #[serde(deserialize_with = "color")]
    pub background: Color,
    #[serde(deserialize_with = "color")]
    pub foreground: Color,
    pub border: Border,
    pub progress: Progress,
    pub action: Action,
    /// color of links in the body
    #[serde(deserialize_with = "color")]
    pub link: Color,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            font: None,
            font_size: 14.0,
            max_lines: 10,
            show_app_name: false,
            padding: 12,
            spacing: 8,
            icon_size: 48,
            min_icon_size: None,
            max_icon_size: None,
            background: Color::from_rgba8(0x1e, 0x1e, 0x2e, 0.9),
            foreground: Color::from_rgb8(0xcd, 0xd6, 0xf4),
            border: Border::default(),
            progress: Progress::default(),
            action: Action::default(),
            link: Color::from_rgb8(0x89, 0xb4, 0xfa),
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
pub struct Progress {
    pub height: u32,
    #[serde(deserialize_with = "color")]
    pub color: Color,
    #[serde(deserialize_with = "color")]
    pub background: Color,
}

impl Default for Progress {
    fn default() -> Self {
        Self {
            height: 6,
            color: Color::from_rgb8(0x89, 0xb4, 0xfa),
            background: Color::from_rgb8(0x45, 0x47, 0x5a),
        }
    }
}

/// buttons for the notification's actions
#[derive(Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Action {
    pub padding: u32,
    #[serde(deserialize_with = "color")]
    pub background: Color,
    #[serde(deserialize_with = "color")]
    pub foreground: Color,
    pub radius: f32,
}

impl Default for Action {
    fn default() -> Self {
        Self {
            padding: 6,
            background: Color::from_rgb8(0x31, 0x32, 0x44),
            foreground: Color::from_rgb8(0xcd, 0xd6, 0xf4),
            radius: 6.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Urgencies {
    pub low: UrgencyConfig,
    pub normal: UrgencyConfig,
    pub critical: UrgencyConfig,
}

impl Urgencies {
    pub fn get(&self, urgency: Urgency) -> &UrgencyConfig {
        match urgency {
            Urgency::Low => &self.low,
            Urgency::Normal => &self.normal,
            Urgency::Critical => &self.critical,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UrgencyConfig {
    pub timeout: Timeout,
}

const DEFAULT_TIMEOUTS: [Timeout; 3] = [
    Timeout(Some(Duration::from_secs(5))),
    Timeout(Some(Duration::from_secs(10))),
    Timeout(None),
];

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
pub fn parse_color(s: &str) -> Result<Color, String> {
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

/// iced wants font families as `&'static str`: each distinct name is leaked
/// once, so config reloads don't leak more.
fn font_family<'de, D: Deserializer<'de>>(d: D) -> Result<Option<&'static str>, D::Error> {
    static NAMES: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let name = String::deserialize(d)?;
    let mut names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    let name = match names.iter().find(|n| **n == name) {
        Some(n) => *n,
        None => {
            let n: &'static str = name.leak();
            names.push(n);
            n
        }
    };
    Ok(Some(name))
}

/// Merges `over` into `base`; nested tables are merged, other values replaced.
/// Dotted keys like `border.color = ...` are nested tables in TOML already.
pub fn merge(base: &mut toml::Table, over: &toml::Table) {
    for (key, value) in over {
        match (base.get_mut(key), value) {
            (Some(toml::Value::Table(b)), toml::Value::Table(o)) => merge(b, o),
            _ => {
                base.insert(key.clone(), value.clone());
            }
        }
    }
}

pub fn default_path() -> PathBuf {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_default();
    config_home.join("durst/config.toml")
}

/// Reads the config file; `None` if it doesn't exist and wasn't given
/// explicitly, then the defaults apply.
pub fn read(path: &Path, explicit: bool) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !explicit => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Loads the config and returns it with the text it came from. A missing
/// file at the default location yields the defaults; every other problem is
/// an error.
pub fn load(path: &Path, explicit: bool) -> Result<(Config, Option<String>), String> {
    let text = read(path, explicit)?;
    let config = match &text {
        Some(s) => parse(s).map_err(|e| format!("{}: {e}", path.display()))?,
        None => {
            log::info!("no config at {}, using defaults", path.display());
            Config::default()
        }
    };
    Ok((config, text))
}

/// A TOML error without the source excerpt (`2 | width = ...` and the
/// `^^^` markers), which only lines up in a monospace terminal.
pub fn short_error(error: &str) -> String {
    let excerpt = |line: &str| {
        let t = line.trim_start();
        t.starts_with('|')
            || t.split_once(" |")
                .is_some_and(|(n, _)| n.parse::<u32>().is_ok())
    };
    error
        .lines()
        .filter(|l| !l.trim().is_empty() && !excerpt(l))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn parse(s: &str) -> Result<Config, String> {
    let raw: RawConfig = toml::from_str(s).map_err(|e| e.to_string())?;
    let urgencies = [&raw.urgency.low, &raw.urgency.normal, &raw.urgency.critical];

    let mut styles = Vec::with_capacity(3);
    let mut style_tables = Vec::with_capacity(3);
    let mut timeouts = Vec::with_capacity(3);
    for (i, (urgency, name)) in urgencies
        .iter()
        .zip(["low", "normal", "critical"])
        .enumerate()
    {
        let mut table = raw.style.clone();
        merge(
            &mut table,
            &toml::from_str(DEFAULT_URGENCY_STYLES[i]).unwrap(),
        );
        merge(&mut table, &urgency.style);
        let style = toml::Value::Table(table.clone())
            .try_into::<Style>()
            .map_err(|e| format!("in [style] or [urgency.{name}.style]: {e}"))?;
        styles.push(style);
        style_tables.push(table);
        timeouts.push(urgency.timeout.unwrap_or(DEFAULT_TIMEOUTS[i]));
    }
    let timeout = |i: usize| UrgencyConfig {
        timeout: timeouts[i],
    };

    let config = Config {
        general: raw.general,
        history: raw.history,
        mouse: raw.mouse,
        urgency: Urgencies {
            low: timeout(0),
            normal: timeout(1),
            critical: timeout(2),
        },
        sound: raw.sound,
        osd: raw.osd,
        rules: raw.rule,
        styles: styles.try_into().unwrap(),
        style_tables: style_tables.try_into().unwrap(),
    };
    // catch typos in rule styles now, not when the rule first matches
    for (i, rule) in config.rules.iter().enumerate() {
        for urgency in [Urgency::Low, Urgency::Normal, Urgency::Critical] {
            config.style_with(urgency, &rule.style).map_err(|e| {
                let name = rule.name.clone().unwrap_or_else(|| format!("#{}", i + 1));
                format!("in the style of rule {name}: {e}")
            })?;
        }
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_is_default() {
        let c = parse("").unwrap();
        assert_eq!(c.general.anchor, Anchor::TopRight);
        assert_eq!(c.urgency.critical.timeout, Timeout(None));
        assert_eq!(c.mouse.right, vec![MouseAction::CloseAll]);
    }

    #[test]
    fn example_config_parses() {
        parse(include_str!("../../contrib/config.toml")).unwrap();
    }

    #[test]
    fn unknown_key_is_reported_with_location() {
        let err = parse("[general]\nwidht = 3\n").unwrap_err();
        assert!(err.contains("widht"), "{err}");
        assert!(err.contains("line 2"), "{err}");
    }

    #[test]
    fn short_errors_drop_the_excerpt() {
        let err = parse("[general]\nwidth = \"wide\"\n").unwrap_err();
        assert_eq!(
            short_error(&err),
            "TOML parse error at line 2, column 9\ninvalid type: string \"wide\", expected u32"
        );
    }

    #[test]
    fn unknown_style_key_names_the_section() {
        let err = parse("[urgency.low.style]\nborder.colr = \"#000000\"\n").unwrap_err();
        assert!(err.contains("colr") && err.contains("urgency.low"), "{err}");
    }

    #[test]
    fn urgency_styles_are_merged_over_the_base() {
        let c = parse(
            r##"
            [style]
            font_size = 20
            border = { width = 3, color = "#000000" }
            [urgency.low.style]
            border.width = 1
            "##,
        )
        .unwrap();
        let low = c.style(Urgency::Low);
        assert_eq!((low.font_size, low.border.width), (20.0, 1));
        assert_eq!(low.border.color, Color::BLACK);
        let normal = c.style(Urgency::Normal);
        assert_eq!(
            (normal.border.width, normal.border.color),
            (3, Color::BLACK)
        );
        // the built-in critical color sits between base and user override
        let critical = c.style(Urgency::Critical);
        assert_eq!(critical.border.color, parse_color("#f38ba8").unwrap());
        assert_eq!(critical.border.width, 3);
    }

    #[test]
    fn rule_styles_are_checked_at_load() {
        let ok = parse("[[rule]]\nstyle = { border.width = 5 }").unwrap();
        let style = ok.style_with(Urgency::Normal, &ok.rules[0].style).unwrap();
        assert_eq!(style.border.width, 5);
        let err = parse("[[rule]]\nname = \"x\"\nstyle = { bordr = 5 }").unwrap_err();
        assert!(err.contains("rule x") && err.contains("bordr"), "{err}");
    }

    #[test]
    fn output() {
        let c = parse("[general]\noutput = \"name:DP-1\"").unwrap();
        assert_eq!(c.general.output, Output::Name("DP-1".into()));
        assert!(parse("[general]\noutput = \"name:\"").is_err());
        assert_eq!(
            parse("[general]\noutput = \"all\"").unwrap().general.output,
            Output::All
        );
        assert!(parse("[general]\noutput = \"left\"").is_err());
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
        assert_eq!(c.urgency.critical.timeout, Timeout(None));
    }

    #[test]
    fn colors() {
        assert_eq!(parse_color("#ff0000"), Ok(Color::from_rgb8(255, 0, 0)));
        assert_eq!(parse_color("#00ff0080").unwrap().a, 128.0 / 255.0);
        assert!(parse_color("ff0000").is_err());
        assert!(parse_color("#ff00").is_err());
    }
}
