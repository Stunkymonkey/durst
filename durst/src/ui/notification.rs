//! Rendering of a single notification, and the prediction of its height.
//!
//! Each notification is its own layer surface whose size must be known
//! before it is opened, so `height` mirrors the layout built by `view`
//! exactly: same fonts, sizes, wrapping and spacing.

use std::path::Path;

use iced::advanced::graphics::text::Paragraph;
use iced::advanced::text::{
    Alignment as TextAlignment, LineHeight, Paragraph as _, Shaping, Text, Wrapping,
};
use iced::alignment::Vertical;
use iced::widget::{column, container, image, mouse_area, row, svg, text};
use iced::{Element, Font, Length, Size, font};

use crate::config::Style;
use crate::core::notification::Notification;

const SHAPING: Shaping = Shaping::Advanced;
const WRAPPING: Wrapping = Wrapping::WordOrGlyph;
const SUMMARY_FONT: Font = Font {
    weight: font::Weight::Bold,
    ..Font::DEFAULT
};
const BODY_FONT: Font = Font::DEFAULT;

/// Inner padding; iced draws borders on top of the content, so the border
/// width is added to keep content off the border.
fn inset(style: &Style) -> u32 {
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

fn text_height(content: &str, font: Font, style: &Style, width: f32) -> f32 {
    let paragraph = Paragraph::with_text(Text {
        content,
        bounds: Size::new(width, f32::INFINITY),
        size: style.font_size.into(),
        line_height: LineHeight::default(),
        font,
        align_x: TextAlignment::Default,
        align_y: Vertical::Top,
        shaping: SHAPING,
        wrapping: WRAPPING,
    });
    paragraph.min_bounds().height
}

/// Height in pixels of the surface `view` renders for this notification.
pub fn height(n: &Notification, has_icon: bool, style: &Style, width: u32) -> u32 {
    let text_w = text_width(style, width, has_icon);
    let parts: Vec<f32> = [(&n.summary, SUMMARY_FONT), (&n.body, BODY_FONT)]
        .into_iter()
        .filter(|(s, _)| !s.is_empty())
        .map(|(s, font)| text_height(s, font, style, text_w))
        .collect();
    let gaps = parts.len().saturating_sub(1) as f32 * style.spacing as f32;
    let text_h = parts.iter().sum::<f32>() + gaps;
    let icon_h = if has_icon {
        style.icon_size as f32
    } else {
        0.0
    };
    (text_h.max(icon_h) + 2.0 * inset(style) as f32).ceil() as u32
}

pub fn view<'a, M: Clone + 'a>(
    n: &'a Notification,
    icon: Option<&'a Path>,
    style: &'a Style,
    on_press: M,
) -> Element<'a, M> {
    let label = |content: &'a str, font| {
        text(content)
            .font(font)
            .size(style.font_size)
            .line_height(LineHeight::default())
            .shaping(SHAPING)
            .wrapping(WRAPPING)
            .width(Length::Fill)
    };
    let mut texts = column![].spacing(style.spacing);
    if !n.summary.is_empty() {
        texts = texts.push(label(&n.summary, SUMMARY_FONT));
    }
    if !n.body.is_empty() {
        texts = texts.push(label(&n.body, BODY_FONT));
    }

    let mut content = row![].spacing(style.spacing);
    if let Some(path) = icon {
        let size = Length::Fixed(style.icon_size as f32);
        let icon: Element<'a, M> = if path.extension().is_some_and(|e| e == "svg") {
            svg(svg::Handle::from_path(path))
                .width(size)
                .height(size)
                .into()
        } else {
            image(image::Handle::from_path(path))
                .width(size)
                .height(size)
                .into()
        };
        content = content.push(icon);
    }
    content = content.push(texts);

    let style = style.clone();
    let surface = container(content)
        .padding(inset(&style) as f32)
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
        });
    mouse_area(surface).on_press(on_press).into()
}
