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
    button, column, container, image, mouse_area, progress_bar, rich_text, row, svg, text,
};
use iced::{Element, Font, Length, Size, font, mouse};

use super::icons::Icon;
use super::markup::Run;
use crate::config::Style;
use crate::core::notification::Notification;

pub(super) const SHAPING: Shaping = Shaping::Advanced;
const WRAPPING: Wrapping = Wrapping::WordOrGlyph;
pub(super) const SUMMARY_FONT: Font = Font {
    weight: font::Weight::Bold,
    ..Font::DEFAULT
};
pub(super) const BODY_FONT: Font = Font::DEFAULT;

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
    /// icon after the text instead of before it
    pub icon_right: bool,
}

impl<'a> Content<'a> {
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

fn text_width(style: &Style, width: u32, has_icon: bool) -> f32 {
    let icon = if has_icon {
        style.icon_size + style.spacing
    } else {
        0
    };
    width.saturating_sub(2 * inset(style) + icon) as f32
}

fn layout<T>(
    content: T,
    font: Font,
    style: &Style,
    width: f32,
    wrapping: Wrapping,
) -> Text<T, Font> {
    Text {
        content,
        bounds: Size::new(width, f32::INFINITY),
        size: style.font_size.into(),
        line_height: LineHeight::default(),
        font,
        align_x: TextAlignment::Default,
        align_y: Vertical::Top,
        shaping: SHAPING,
        wrapping,
    }
}

/// Height of plain text; `width = INFINITY` means a single line.
/// Width of a single line of text.
pub(super) fn line_width(content: &str, font: Font, style: &Style) -> f32 {
    Paragraph::with_text(layout(content, font, style, f32::INFINITY, Wrapping::None))
        .min_bounds()
        .width
}

pub(super) fn text_height(content: &str, font: Font, style: &Style, width: f32) -> f32 {
    let wrapping = if width.is_finite() {
        WRAPPING
    } else {
        Wrapping::None
    };
    Paragraph::with_text(layout(content, font, style, width, wrapping))
        .min_bounds()
        .height
}

fn spans_height(spans: &[Span<'_, String, Font>], style: &Style, width: f32) -> f32 {
    Paragraph::with_spans(layout(spans, BODY_FONT, style, width, WRAPPING))
        .min_bounds()
        .height
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
                ..BODY_FONT
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

fn action_height(c: &Content, style: &Style) -> Option<f32> {
    let line = c
        .actions()
        .map(|(_, label)| text_height(label, BODY_FONT, style, f32::INFINITY))
        .reduce(f32::max)?;
    Some(line + 2.0 * style.action.padding as f32)
}

/// Height in pixels of the surface `view` renders.
pub fn height(c: &Content, style: &Style, width: u32) -> u32 {
    let text_w = text_width(style, width, c.icon.is_some());
    let summary = c.summary();
    let parts: Vec<f32> = [
        (!summary.is_empty()).then(|| text_height(&summary, SUMMARY_FONT, style, text_w)),
        (!c.body.is_empty()).then(|| spans_height(&body_spans(c.body, style), style, text_w)),
        c.notification
            .hints
            .value
            .map(|_| style.progress.height as f32),
        action_height(c, style),
    ]
    .into_iter()
    .flatten()
    .collect();
    let gaps = parts.len().saturating_sub(1) as f32 * style.spacing as f32;
    let text_h = parts.iter().sum::<f32>() + gaps;
    let icon_h = if c.icon.is_some() {
        style.icon_size as f32
    } else {
        0.0
    };
    (text_h.max(icon_h) + 2.0 * inset(style) as f32).ceil() as u32
}

pub fn view<'a>(c: Content<'a>, style: &'a Style) -> Element<'a, Event> {
    let mut texts = column![].spacing(style.spacing);
    let summary = c.summary();
    if !summary.is_empty() {
        texts = texts.push(
            text(summary)
                .font(SUMMARY_FONT)
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
                .font(BODY_FONT)
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
    let actions: Vec<Element<'a, Event>> = c
        .actions()
        .map(|(key, label)| {
            let action = style.action.clone();
            button(
                text(label.as_str())
                    .font(BODY_FONT)
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
        texts = texts.push(row(actions).spacing(style.spacing));
    }

    let icon = c.icon.map(|icon| {
        let size = Length::Fixed(style.icon_size as f32);
        match icon {
            Icon::Svg(path) => {
                Element::from(svg(svg::Handle::from_path(path)).width(size).height(size))
            }
            Icon::Raster(handle) => image(handle.clone()).width(size).height(size).into(),
        }
    });
    let content = match (icon, c.icon_right) {
        (None, _) => row![texts],
        (Some(icon), false) => row![icon, texts],
        (Some(icon), true) => row![texts, icon],
    }
    .spacing(style.spacing);

    mouse_area(frame(content.into(), style, inset(style)))
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
    let h = text_height(&more_text(waiting), BODY_FONT, style, f32::INFINITY);
    (h + 2.0 * more_inset(style) as f32).ceil() as u32
}

pub fn more_view(waiting: usize, style: &Style) -> Element<'_, Event> {
    let label = text(more_text(waiting))
        .font(BODY_FONT)
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
