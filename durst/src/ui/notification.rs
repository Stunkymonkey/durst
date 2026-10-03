//! Rendering of a single notification, and the prediction of its height.
//!
//! Each notification is its own layer surface whose size must be known
//! before it is opened, so `height` mirrors the layout built by `view`
//! exactly: same fonts, sizes, wrapping and spacing. Anything added to `view`
//! must be added to `height` as well (and covered by a visual scenario).

use iced::advanced::graphics::text::Paragraph;
use iced::advanced::text::{
    Alignment as TextAlignment, LineHeight, Paragraph as _, Shaping, Span, Text, Wrapping,
};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{
    button, column, container, image, mouse_area, progress_bar, rich_text, row, space, svg, text,
};
use iced::{Element, Font, Length, Size, font, mouse};

use super::icons::Icon;
use super::markup::Run;
use crate::config::Style;
use crate::core::notification::Notification;
use crate::core::rules::IconPosition;

pub(super) const SHAPING: Shaping = Shaping::Advanced;
const WRAPPING: Wrapping = Wrapping::WordOrGlyph;

/// The style's font, or the system's sans-serif.
pub(super) fn body_font(style: &Style) -> Font {
    Font {
        family: style
            .font
            .map_or(font::Family::SansSerif, font::Family::Name),
        ..Font::DEFAULT
    }
}

pub(super) fn summary_font(style: &Style) -> Font {
    Font {
        weight: font::Weight::Bold,
        ..body_font(style)
    }
}

fn app_name_size(style: &Style) -> f32 {
    (style.font_size * 0.85).round()
}

/// What a notification surface reports back.
#[derive(Debug, Clone)]
pub enum Event {
    Press(mouse::Button),
    /// vertical scroll, positive is up
    Scroll(f32),
    Hover(bool),
    Action(String),
    Link(String),
}

/// Everything a notification surface shows.
pub struct Content<'a> {
    pub notification: &'a Notification,
    pub count: u32,
    pub body: &'a [Run],
    pub icon: Option<&'a Icon>,
    /// left, right or above the text (`Off` doesn't get an icon)
    pub icon_position: IconPosition,
}

/// The width of an icon beside the text, which the text can't use.
pub fn side_icon_width(icon: Option<&Icon>, position: IconPosition) -> Option<u32> {
    icon.filter(|_| position != IconPosition::Top)
        .map(Icon::width)
}

impl<'a> Content<'a> {
    fn side_icon_width(&self) -> Option<u32> {
        side_icon_width(self.icon, self.icon_position)
    }

    /// the app name line, if the style shows it
    fn app_name(&self, style: &Style) -> Option<&'a str> {
        let notification: &'a Notification = self.notification;
        (style.show_app_name && !notification.app_name.is_empty())
            .then_some(notification.app_name.as_str())
    }

    fn summary(&self) -> String {
        match self.count {
            1 => self.notification.summary.clone(),
            n => format!("{} ({n})", self.notification.summary),
        }
    }

    /// action buttons; the "default" action is triggered by clicking the
    /// notification itself
    fn actions(&self) -> impl Iterator<Item = &'a (String, String)> + 'a {
        let notification: &'a Notification = self.notification;
        notification
            .actions
            .iter()
            .filter(|(key, _)| key != "default")
    }
}

/// Inner padding; iced draws borders on top of the content, so the border
/// width is added to keep content off the border.
pub(super) fn inset(style: &Style) -> u32 {
    style.padding + style.border.width
}

fn text_width(style: &Style, width: u32, side_icon: Option<u32>) -> f32 {
    let icon = side_icon.map_or(0, |w| w + style.spacing);
    width.saturating_sub(2 * inset(style) + icon) as f32
}

fn layout<T>(content: T, font: Font, size: f32, width: f32, wrapping: Wrapping) -> Text<T, Font> {
    Text {
        content,
        bounds: Size::new(width, f32::INFINITY),
        size: size.into(),
        line_height: LineHeight::default(),
        font,
        align_x: TextAlignment::Default,
        align_y: Vertical::Top,
        shaping: SHAPING,
        wrapping,
    }
}

/// Width of a single line of text.
pub(super) fn line_width(content: &str, font: Font, style: &Style) -> f32 {
    Paragraph::with_text(layout(
        content,
        font,
        style.font_size,
        f32::INFINITY,
        Wrapping::None,
    ))
    .min_bounds()
    .width
}

/// Height of plain text; `width = INFINITY` means a single line.
pub(super) fn text_height(content: &str, font: Font, style: &Style, width: f32) -> f32 {
    sized_text_height(content, font, style.font_size, width)
}

fn sized_text_height(content: &str, font: Font, size: f32, width: f32) -> f32 {
    let wrapping = if width.is_finite() {
        WRAPPING
    } else {
        Wrapping::None
    };
    Paragraph::with_text(layout(content, font, size, width, wrapping))
        .min_bounds()
        .height
}

fn spans_height(spans: &[Span<'_, String, Font>], style: &Style, width: f32) -> f32 {
    Paragraph::with_spans(layout(
        spans,
        body_font(style),
        style.font_size,
        width,
        WRAPPING,
    ))
    .min_bounds()
    .height
}

/// Cuts the body to `style.max_lines` lines at the width it gets, ending it
/// with "…". Done once when a notification is prepared, so `height` and
/// `view` see the same cut body.
pub fn fit_body(runs: Vec<Run>, style: &Style, width: u32, side_icon: Option<u32>) -> Vec<Run> {
    if style.max_lines == 0 || runs.is_empty() {
        return runs;
    }
    let text_w = text_width(style, width, side_icon);
    let line = LineHeight::default().to_absolute(style.font_size.into()).0;
    let max_h = style.max_lines as f32 * line + line / 2.0;
    let fits = |runs: &[Run]| spans_height(&body_spans(runs, style), style, text_w) <= max_h;
    let total: usize = runs.iter().map(|r| r.text.chars().count()).sum();
    // no line holds more characters than this (narrowest glyphs ~0.2 em),
    // which keeps a huge body from being laid out in full
    let per_line = (text_w / (style.font_size * 0.2)).ceil() as usize + 1;
    let bound = per_line.saturating_mul(style.max_lines as usize);
    if total <= bound && fits(&runs) {
        return runs;
    }
    // the longest cut that fits; 0 characters (only "…") always does
    let (mut lo, mut hi) = (0, total.min(bound));
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if fits(&cut(&runs, mid)) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    cut(&runs, lo)
}

/// The first `chars` characters of the runs, without trailing whitespace,
/// followed by "…".
fn cut(runs: &[Run], chars: usize) -> Vec<Run> {
    let mut out = Vec::new();
    let mut left = chars;
    for run in runs {
        if left == 0 {
            break;
        }
        let n = run.text.chars().count();
        let text = if n <= left {
            run.text.clone()
        } else {
            run.text.chars().take(left).collect()
        };
        left -= n.min(left);
        out.push(Run {
            text,
            ..run.clone()
        });
    }
    while let Some(last) = out.last_mut() {
        last.text.truncate(last.text.trim_end().len());
        if !last.text.is_empty() {
            break;
        }
        out.pop();
    }
    out.push(Run {
        text: "…".into(),
        ..Run::default()
    });
    out
}

fn body_spans<'a>(runs: &'a [Run], style: &Style) -> Vec<Span<'a, String, Font>> {
    runs.iter()
        .map(|run| {
            let font = Font {
                weight: if run.bold {
                    font::Weight::Bold
                } else {
                    font::Weight::Normal
                },
                style: if run.italic {
                    font::Style::Italic
                } else {
                    font::Style::Normal
                },
                ..body_font(style)
            };
            let span = Span::new(run.text.as_str())
                .font(font)
                .underline(run.underline || run.link.is_some());
            match &run.link {
                Some(link) => span.link(link.clone()).color(style.link),
                None => span,
            }
        })
        .collect()
}

/// How many action buttons share a row: all buttons are equally wide, and
/// as many as fit with their labels uncut; at least one (a label wider than
/// the whole row is cut).
fn actions_per_row(c: &Content, style: &Style, text_w: f32) -> usize {
    let widest = c
        .actions()
        .map(|(_, label)| line_width(label, body_font(style), style).ceil())
        .fold(0.0, f32::max)
        + 2.0 * style.action.padding as f32;
    let spacing = style.spacing as f32;
    let n = c.actions().count();
    (1..=n)
        .rev()
        .find(|&k| (text_w - (k - 1) as f32 * spacing) / k as f32 >= widest)
        .unwrap_or(1)
}

fn action_height(c: &Content, style: &Style, text_w: f32) -> Option<f32> {
    let line = c
        .actions()
        .map(|(_, label)| text_height(label, body_font(style), style, f32::INFINITY))
        .reduce(f32::max)?;
    let rows = c
        .actions()
        .count()
        .div_ceil(actions_per_row(c, style, text_w));
    let button = line + 2.0 * style.action.padding as f32;
    Some(rows as f32 * button + (rows - 1) as f32 * style.spacing as f32)
}

/// Height in pixels of the surface `view` renders.
pub fn height(c: &Content, style: &Style, width: u32) -> u32 {
    let text_w = text_width(style, width, c.side_icon_width());
    let summary = c.summary();
    let parts: Vec<f32> = [
        c.app_name(style)
            .map(|name| sized_text_height(name, body_font(style), app_name_size(style), text_w)),
        (!summary.is_empty()).then(|| text_height(&summary, summary_font(style), style, text_w)),
        (!c.body.is_empty()).then(|| spans_height(&body_spans(c.body, style), style, text_w)),
        c.notification
            .hints
            .value
            .map(|_| style.progress.height as f32),
        action_height(c, style, text_w),
    ]
    .into_iter()
    .flatten()
    .collect();
    let gaps = parts.len().saturating_sub(1) as f32 * style.spacing as f32;
    let text_h = parts.iter().sum::<f32>() + gaps;
    let icon_h = c.icon.map_or(0.0, |icon| icon.height() as f32);
    let content_h = if c.icon_position == IconPosition::Top && c.icon.is_some() {
        icon_h + style.spacing as f32 + text_h
    } else {
        text_h.max(icon_h)
    };
    (content_h + 2.0 * inset(style) as f32).ceil() as u32
}

pub fn view<'a>(c: Content<'a>, style: &'a Style, width: u32) -> Element<'a, Event> {
    let mut texts = column![].spacing(style.spacing);
    if let Some(name) = c.app_name(style) {
        texts = texts.push(
            text(name)
                .font(body_font(style))
                .size(app_name_size(style))
                .line_height(LineHeight::default())
                .shaping(SHAPING)
                .wrapping(WRAPPING)
                .color(style.foreground.scale_alpha(0.7))
                .width(Length::Fill),
        );
    }
    let summary = c.summary();
    if !summary.is_empty() {
        texts = texts.push(
            text(summary)
                .font(summary_font(style))
                .size(style.font_size)
                .line_height(LineHeight::default())
                .shaping(SHAPING)
                .wrapping(WRAPPING)
                .width(Length::Fill),
        );
    }
    if !c.body.is_empty() {
        texts = texts.push(
            rich_text(body_spans(c.body, style))
                .font(body_font(style))
                .size(style.font_size)
                .line_height(LineHeight::default())
                .wrapping(WRAPPING)
                .width(Length::Fill)
                .on_link_click(Event::Link),
        );
    }
    if let Some(value) = c.notification.hints.value {
        let progress = style.progress.clone();
        texts = texts.push(
            progress_bar(0.0..=100.0, value as f32)
                .girth(progress.height as f32)
                .style(move |_| progress_bar::Style {
                    background: progress.background.into(),
                    bar: progress.color.into(),
                    border: iced::Border::default().rounded(progress.height as f32 / 2.0),
                }),
        );
    }
    let per_row = actions_per_row(&c, style, text_width(style, width, c.side_icon_width()));
    let mut actions: Vec<Element<'a, Event>> = c
        .actions()
        .map(|(key, label)| {
            let action = style.action.clone();
            button(
                text(label.as_str())
                    .font(body_font(style))
                    .size(style.font_size)
                    .line_height(LineHeight::default())
                    .shaping(SHAPING)
                    .wrapping(Wrapping::None)
                    .align_x(Horizontal::Center)
                    .width(Length::Fill),
            )
            .padding(action.padding as f32)
            .width(Length::Fill)
            .on_press(Event::Action(key.clone()))
            .style(move |_, status| button::Style {
                background: Some(match status {
                    button::Status::Hovered | button::Status::Pressed => {
                        action.background.scale_alpha(0.7).into()
                    }
                    _ => action.background.into(),
                }),
                text_color: action.foreground,
                border: iced::Border::default().rounded(action.radius),
                ..Default::default()
            })
            .into()
        })
        .collect();
    if !actions.is_empty() {
        // fill the last row up, so the buttons stay in a grid
        let missing = actions.len().next_multiple_of(per_row) - actions.len();
        actions.extend((0..missing).map(|_| space().width(Length::Fill).into()));
        let mut grid = column![].spacing(style.spacing);
        let mut actions = actions.into_iter().peekable();
        while actions.peek().is_some() {
            grid = grid.push(row(actions.by_ref().take(per_row)).spacing(style.spacing));
        }
        texts = texts.push(grid);
    }

    let content: Element<'a, Event> = match (c.icon.map(icon_view), c.icon_position) {
        (None, _) => texts.into(),
        (Some(icon), IconPosition::Top) => column![container(icon).center_x(Length::Fill), texts]
            .spacing(style.spacing)
            .into(),
        (Some(icon), IconPosition::Right) => row![texts, icon].spacing(style.spacing).into(),
        (Some(icon), _) => row![icon, texts].spacing(style.spacing).into(),
    };

    mouse_area(frame(content, style, inset(style)))
        .on_press(Event::Press(mouse::Button::Left))
        .on_middle_press(Event::Press(mouse::Button::Middle))
        .on_right_press(Event::Press(mouse::Button::Right))
        .on_scroll(|delta| {
            Event::Scroll(match delta {
                mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. } => y,
            })
        })
        .on_enter(Event::Hover(true))
        .on_exit(Event::Hover(false))
        .into()
}

/// An icon drawn at its size.
pub(super) fn icon_view<'a, E: 'a>(icon: &'a Icon) -> Element<'a, E> {
    let (w, h) = (
        Length::Fixed(icon.width() as f32),
        Length::Fixed(icon.height() as f32),
    );
    match icon {
        Icon::Svg { path, .. } => svg(svg::Handle::from_path(path)).width(w).height(h).into(),
        Icon::Raster { handle, .. } => image(handle.clone()).width(w).height(h).into(),
    }
}

/// The surface's box: background, border and padding.
pub(super) fn frame<'a, E: 'a>(
    content: Element<'a, E>,
    style: &Style,
    padding: u32,
) -> Element<'a, E> {
    let style = style.clone();
    container(content)
        .padding(padding as f32)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container::Style {
            text_color: Some(style.foreground),
            background: Some(style.background.into()),
            border: iced::Border {
                color: style.border.color,
                width: style.border.width as f32,
                radius: style.border.radius.into(),
            },
            ..Default::default()
        })
        .into()
}

fn more_text(waiting: usize) -> String {
    format!("+{waiting} more")
}

fn more_inset(style: &Style) -> u32 {
    style.padding / 2 + style.border.width
}

/// Height of the "+N more" indicator below the stack.
pub fn more_height(waiting: usize, style: &Style) -> u32 {
    let h = text_height(&more_text(waiting), body_font(style), style, f32::INFINITY);
    (h + 2.0 * more_inset(style) as f32).ceil() as u32
}

pub fn more_view(waiting: usize, style: &Style) -> Element<'_, Event> {
    let label = text(more_text(waiting))
        .font(body_font(style))
        .size(style.font_size)
        .line_height(LineHeight::default())
        .shaping(SHAPING)
        .wrapping(Wrapping::None)
        .align_x(Horizontal::Center)
        .width(Length::Fill);
    mouse_area(frame(label.into(), style, more_inset(style)))
        .on_press(Event::Press(mouse::Button::Left))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, bold: bool) -> Run {
        Run {
            text: text.into(),
            bold,
            ..Run::default()
        }
    }

    #[test]
    fn cut_keeps_formatting_and_ends_with_ellipsis() {
        let runs = [run("plain ", false), run("bold text", true)];
        // inside the second run: its format stays
        assert_eq!(
            cut(&runs, 10),
            vec![run("plain ", false), run("bold", true), run("…", false)]
        );
        // trailing whitespace before the ellipsis goes, empty runs too
        assert_eq!(cut(&runs, 6), vec![run("plain", false), run("…", false)]);
        assert_eq!(cut(&runs, 0), vec![run("…", false)]);
    }
}
