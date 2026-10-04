//! `[[rule]]`s: match notifications and change them, dunst-style.
//!
//! Rules are applied in order; each one sees the notification as the earlier
//! rules left it, and later rules override earlier ones. Text matchers are
//! regexes searched anywhere in the field (use `^...$` to anchor them).

use std::collections::BTreeSet;

use regex::Regex;
use serde::{Deserialize, Deserializer};

use super::notification::{Notification, Urgency};
use crate::config::{Anchor, Output, Timeout};

#[derive(Debug, Clone)]
pub struct Pattern(Regex);

impl<'de> Deserialize<'de> for Pattern {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Regex::new(&s)
            .map(Pattern)
            .map_err(serde::de::Error::custom)
    }
}

impl Pattern {
    fn is_match(&self, s: &str) -> bool {
        self.0.is_match(s)
    }
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UrgencyName {
    Low,
    Normal,
    Critical,
}

impl From<UrgencyName> for Urgency {
    fn from(u: UrgencyName) -> Self {
        match u {
            UrgencyName::Low => Urgency::Low,
            UrgencyName::Normal => Urgency::Normal,
            UrgencyName::Critical => Urgency::Critical,
        }
    }
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum IconPosition {
    #[default]
    Left,
    Right,
    /// above the text, centered
    Top,
    Off,
}

/// What happens while a fullscreen window has the focus.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Fullscreen {
    /// show it anyway
    #[default]
    Show,
    /// hold back notifications arriving during fullscreen until it ends
    Delay,
    /// hold back while fullscreen, even ones already shown
    Pushback,
}

/// The state the rules can match on, besides the notification itself.
#[derive(Debug, Clone, Default)]
pub struct Context {
    pub modes: BTreeSet<String>,
    /// a focused window is fullscreen
    pub fullscreen: bool,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ScriptOn {
    /// when the notification arrives
    #[default]
    Receive,
    /// when one of its actions is invoked
    Action,
    /// when it is closed, for any reason
    Close,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Rule {
    /// shown in the debug log when the rule matches
    pub name: Option<String>,

    // matchers: all given ones must match
    app_name: Option<Pattern>,
    not_app_name: Option<Pattern>,
    summary: Option<Pattern>,
    not_summary: Option<Pattern>,
    body: Option<Pattern>,
    not_body: Option<Pattern>,
    icon: Option<Pattern>,
    not_icon: Option<Pattern>,
    category: Option<Pattern>,
    not_category: Option<Pattern>,
    desktop_entry: Option<Pattern>,
    not_desktop_entry: Option<Pattern>,
    stack_tag: Option<Pattern>,
    not_stack_tag: Option<Pattern>,
    urgency: Option<UrgencyName>,
    not_urgency: Option<UrgencyName>,
    transient: Option<bool>,
    has_actions: Option<bool>,
    /// the mode is active
    mode: Option<String>,
    /// the mode is not active
    not_mode: Option<String>,
    /// a focused window is fullscreen
    fullscreen_active: Option<bool>,

    // content; templates may use {app_name} {summary} {body} {category} {urgency}
    set_summary: Option<String>,
    set_body: Option<String>,
    hide_body: Option<bool>,
    set_urgency: Option<UrgencyName>,
    set_category: Option<String>,
    set_stack_tag: Option<String>,
    /// replaces the icon (name or path)
    set_icon: Option<String>,
    set_transient: Option<bool>,

    // presentation
    /// used when the notification has no icon
    default_icon: Option<String>,
    icon_position: Option<IconPosition>,
    /// a stack of its own in this corner and/or on this output
    anchor: Option<Anchor>,
    output: Option<Output>,
    /// overrides for the style, like `[urgency.*.style]`
    pub style: toml::Table,
    timeout: Option<Timeout>,

    // visibility
    /// show | delay | pushback while a fullscreen window has the focus
    fullscreen: Option<Fullscreen>,
    /// straight into the history, never shown
    skip_display: Option<bool>,
    history_ignore: Option<bool>,
    /// hold back while the rule matches (typically with `mode`)
    defer: Option<bool>,

    // actions
    /// the action invoked by `do_action` (middle click)
    default_action: Option<String>,
    /// invoke this action as soon as the notification arrives
    auto_invoke: Option<String>,

    // side effects
    /// command and arguments, the notification is passed as DURST_* env vars
    script: Option<Vec<String>>,
    script_on: ScriptOn,
    /// sound file path or sound theme name
    sound: Option<String>,
    /// don't play any sound, not even one from the notification's hints
    mute_sound: Option<bool>,
}

/// What the matching rules decided, besides the changes to the notification.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    /// names (or indices) of the matching rules
    pub matched: Vec<String>,
    /// all matching rules' style overrides, merged in order
    pub style: toml::Table,
    pub default_icon: Option<String>,
    pub icon_position: IconPosition,
    /// `None`: `[general]`'s
    pub anchor: Option<Anchor>,
    pub output: Option<Output>,
    pub timeout: Option<Timeout>,
    pub skip_display: bool,
    pub history_ignore: bool,
    pub defer: bool,
    pub fullscreen: Fullscreen,
    pub default_action: Option<String>,
    pub auto_invoke: Option<String>,
    pub scripts: Vec<(ScriptOn, Vec<String>)>,
    pub sound: Option<String>,
    pub mute_sound: bool,
}

impl Rule {
    /// the modes this rule refers to
    pub fn modes(&self) -> impl Iterator<Item = &str> {
        self.mode.iter().chain(&self.not_mode).map(String::as_str)
    }

    fn matches(&self, n: &Notification, ctx: &Context) -> bool {
        let modes = &ctx.modes;
        fn text(p: &Option<Pattern>, not: &Option<Pattern>, value: &str) -> bool {
            p.as_ref().is_none_or(|p| p.is_match(value))
                && not.as_ref().is_none_or(|p| !p.is_match(value))
        }
        let h = &n.hints;
        let opt = |s: &Option<String>| s.clone().unwrap_or_default();
        text(&self.app_name, &self.not_app_name, &n.app_name)
            && text(&self.summary, &self.not_summary, &n.summary)
            && text(&self.body, &self.not_body, &n.body)
            && text(&self.icon, &self.not_icon, &n.app_icon)
            && text(&self.category, &self.not_category, &opt(&h.category))
            && text(
                &self.desktop_entry,
                &self.not_desktop_entry,
                &opt(&h.desktop_entry),
            )
            && text(&self.stack_tag, &self.not_stack_tag, &opt(&h.stack_tag))
            && self.urgency.is_none_or(|u| h.urgency == u.into())
            && self.not_urgency.is_none_or(|u| h.urgency != u.into())
            && self.transient.is_none_or(|t| h.transient == t)
            && self.has_actions.is_none_or(|a| n.actions.is_empty() != a)
            && self.mode.as_ref().is_none_or(|m| modes.contains(m))
            && self.not_mode.as_ref().is_none_or(|m| !modes.contains(m))
            && self.fullscreen_active.is_none_or(|f| ctx.fullscreen == f)
    }

    fn apply(&self, n: &mut Notification, out: &mut Outcome) {
        let original = n.clone();
        let expand = |template: &str| expand(template, &original);
        if let Some(t) = &self.set_summary {
            n.summary = expand(t);
        }
        if let Some(t) = &self.set_body {
            n.body = expand(t);
        }
        if self.hide_body == Some(true) {
            n.body.clear();
        }
        if let Some(u) = self.set_urgency {
            n.hints.urgency = u.into();
        }
        if let Some(c) = &self.set_category {
            n.hints.category = Some(c.clone());
        }
        if let Some(tag) = &self.set_stack_tag {
            n.hints.stack_tag = Some(tag.clone());
        }
        if let Some(icon) = &self.set_icon {
            n.app_icon = icon.clone();
            n.hints.image_data = None;
            n.hints.image_path = None;
        }
        if let Some(t) = self.set_transient {
            n.hints.transient = t;
        }

        crate::config::merge(&mut out.style, &self.style);
        set(&mut out.default_icon, &self.default_icon);
        if let Some(p) = self.icon_position {
            out.icon_position = p;
        }
        if self.anchor.is_some() {
            out.anchor = self.anchor;
        }
        set(&mut out.output, &self.output);
        if self.timeout.is_some() {
            out.timeout = self.timeout;
        }
        flag(&mut out.skip_display, self.skip_display);
        flag(&mut out.history_ignore, self.history_ignore);
        flag(&mut out.defer, self.defer);
        if let Some(f) = self.fullscreen {
            out.fullscreen = f;
        }
        set(&mut out.default_action, &self.default_action);
        set(&mut out.auto_invoke, &self.auto_invoke);
        if let Some(cmd) = self.script.clone().filter(|c| !c.is_empty()) {
            out.scripts.push((self.script_on, cmd));
        }
        set(&mut out.sound, &self.sound);
        flag(&mut out.mute_sound, self.mute_sound);
    }
}

fn set<T: Clone>(target: &mut Option<T>, value: &Option<T>) {
    if value.is_some() {
        target.clone_from(value);
    }
}

fn flag(target: &mut bool, value: Option<bool>) {
    if let Some(v) = value {
        *target = v;
    }
}

/// Replaces `{app_name}`, `{summary}`, `{body}`, `{category}`, `{urgency}`.
fn expand(template: &str, n: &Notification) -> String {
    template
        .replace("{app_name}", &n.app_name)
        .replace("{summary}", &n.summary)
        .replace("{body}", &n.body)
        .replace("{category}", n.hints.category.as_deref().unwrap_or(""))
        .replace("{urgency}", n.hints.urgency.name())
}

/// Applies all matching rules to `n` and returns what they decided.
pub fn apply(rules: &[Rule], n: &mut Notification, ctx: &Context) -> Outcome {
    let mut out = Outcome::default();
    for (i, rule) in rules.iter().enumerate() {
        if rule.matches(n, ctx) {
            out.matched
                .push(rule.name.clone().unwrap_or_else(|| format!("#{}", i + 1)));
            rule.apply(n, &mut out);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::notification::test_notification;

    fn rules(toml: &str) -> Vec<Rule> {
        #[derive(Deserialize)]
        struct R {
            rule: Vec<Rule>,
        }
        toml::from_str::<R>(toml).unwrap().rule
    }

    fn modes(m: &[&str]) -> Context {
        Context {
            modes: m.iter().map(|s| s.to_string()).collect(),
            fullscreen: false,
        }
    }

    #[test]
    fn fullscreen_matcher_and_policy() {
        let r = rules("[[rule]]\nfullscreen_active = true\nfullscreen = \"delay\"");
        let mut ctx = modes(&[]);
        assert_eq!(
            apply(&r, &mut test_notification(1), &ctx).fullscreen,
            Fullscreen::Show
        );
        ctx.fullscreen = true;
        assert_eq!(
            apply(&r, &mut test_notification(1), &ctx).fullscreen,
            Fullscreen::Delay
        );
    }

    #[test]
    fn matchers_combine() {
        let r = rules(
            r#"
            [[rule]]
            app_name = "^app$"
            not_summary = "ignore"
            urgency = "normal"
            has_actions = false
            "#,
        );
        let n = test_notification(1);
        assert!(r[0].matches(&n, &modes(&[])));
        let mut other = n.clone();
        other.summary = "please ignore me".into();
        assert!(!r[0].matches(&other, &modes(&[])));
        let mut other = n.clone();
        other.app_name = "app2".into();
        assert!(!r[0].matches(&other, &modes(&[])));
        let mut other = n.clone();
        other.actions = vec![("default".into(), "Open".into())];
        assert!(!r[0].matches(&other, &modes(&[])));
    }

    #[test]
    fn modes_and_defer() {
        let r = rules(
            r#"
            [[rule]]
            mode = "dnd"
            not_urgency = "critical"
            defer = true
            "#,
        );
        let mut n = test_notification(1);
        assert!(!apply(&r, &mut n.clone(), &modes(&[])).defer);
        assert!(apply(&r, &mut n, &modes(&["dnd"])).defer);
        n.hints.urgency = Urgency::Critical;
        assert!(!apply(&r, &mut n, &modes(&["dnd"])).defer);
    }

    #[test]
    fn rewrites_and_later_rules_see_them() {
        let r = rules(
            r##"
            [[rule]]
            name = "secret"
            app_name = "^app$"
            set_summary = "[{app_name}] {summary}"
            hide_body = true
            set_urgency = "critical"

            [[rule]]
            urgency = "critical"
            timeout = "1s"
            style = { border.color = "#ff0000" }

            [[rule]]
            app_name = "nomatch"
            timeout = "9s"
            "##,
        );
        let mut n = test_notification(1);
        let out = apply(&r, &mut n, &modes(&[]));
        assert_eq!(n.summary, "[app] summary 1");
        assert_eq!(n.body, "");
        assert_eq!(n.hints.urgency, Urgency::Critical);
        assert_eq!(out.matched, vec!["secret", "#2"]);
        assert_eq!(
            out.timeout.unwrap().0,
            Some(std::time::Duration::from_secs(1))
        );
        assert!(out.style.contains_key("border"));
    }

    #[test]
    fn later_rules_override_flags_and_collect_scripts() {
        let r = rules(
            r#"
            [[rule]]
            skip_display = true
            script = ["a"]
            sound = "bell"

            [[rule]]
            skip_display = false
            script = ["b", "--x"]
            script_on = "close"
            mute_sound = true
            "#,
        );
        let out = apply(&r, &mut test_notification(1), &modes(&[]));
        assert!(!out.skip_display);
        assert_eq!(
            out.scripts,
            vec![
                (ScriptOn::Receive, vec!["a".to_string()]),
                (ScriptOn::Close, vec!["b".to_string(), "--x".to_string()])
            ]
        );
        assert_eq!(out.sound.as_deref(), Some("bell"));
        assert!(out.mute_sound);
    }

    #[test]
    fn set_icon_replaces_image_hints() {
        let r = rules("[[rule]]\nset_icon = \"mail\"\nicon_position = \"right\"");
        let mut n = test_notification(1);
        n.hints.image_path = Some("/x.png".into());
        let out = apply(&r, &mut n, &modes(&[]));
        assert_eq!((n.app_icon.as_str(), n.hints.image_path), ("mail", None));
        assert_eq!(out.icon_position, IconPosition::Right);
    }

    #[test]
    fn placement() {
        let r = rules(
            "[[rule]]\nurgency = \"critical\"\nanchor = \"bottom\"\n\
             [[rule]]\napp_name = \"^x$\"\noutput = \"name:DP-2\"",
        );
        let mut n = test_notification(1);
        let out = apply(&r, &mut n, &modes(&[]));
        assert_eq!(
            (out.anchor, out.output),
            (None, None),
            "no match: general's"
        );
        n.hints.urgency = Urgency::Critical;
        n.app_name = "x".into();
        let out = apply(&r, &mut n, &modes(&[]));
        assert_eq!(out.anchor, Some(Anchor::Bottom));
        assert_eq!(out.output, Some(Output::Name("DP-2".into())));
    }

    #[test]
    fn invalid_regex_is_an_error() {
        #[derive(Deserialize, Debug)]
        #[allow(dead_code)]
        struct R {
            rule: Vec<Rule>,
        }
        let err = toml::from_str::<R>("[[rule]]\napp_name = \"(\"").unwrap_err();
        assert!(err.to_string().contains("regex"), "{err}");
    }
}
