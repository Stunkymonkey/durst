//! The volume OSD: device name, slider, percent, mute button.
//!
//! Like notifications it is a layer surface whose height is predicted, so
//! `volume_height` mirrors `volume_view`.

use iced::advanced::text::{LineHeight, Wrapping};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, column, mouse_area, row, slider, text};
use iced::{Element, Length, mouse};

use super::notification::{
    BODY_FONT, SHAPING, SUMMARY_FONT, frame, inset, line_width, text_height,
};
use crate::config::Style;

const SLIDER_HEIGHT: f32 = 16.0;
const PERCENT_WIDTH: f32 = 64.0;

#[derive(Debug, Clone)]
pub enum Event {
    /// the slider is being dragged to this percent
    Slide(f32),
    /// the slider was let go
    Release,
    /// vertical scroll, positive is up
    Scroll(f32),
    ToggleMute,
    Hover(bool),
}

pub struct VolumeContent<'a> {
    pub title: &'a str,
    pub percent: u32,
    pub muted: bool,
    /// the slider's maximum in percent
    pub max: u32,
}

fn percent_label(c: &VolumeContent) -> String {
    match c.muted {
        true => "muted".into(),
        false => format!("{} %", c.percent),
    }
}

fn mute_label(c: &VolumeContent) -> &'static str {
    match c.muted {
        true => "Unmute",
        false => "Mute",
    }
}

/// Wide enough for both labels, so the slider doesn't jump when muting.
fn mute_width(style: &Style) -> f32 {
    let widest = ["Mute", "Unmute"]
        .map(|l| line_width(l, BODY_FONT, style))
        .into_iter()
        .fold(0.0, f32::max);
    (widest + 2.0 * style.action.padding as f32).ceil()
}

/// Height in pixels of the surface `volume_view` renders.
pub fn volume_height(c: &VolumeContent, style: &Style) -> u32 {
    let line = |s: &str, font| text_height(s, font, style, f32::INFINITY);
    let title = line(c.title, SUMMARY_FONT);
    let button = line(mute_label(c), BODY_FONT) + 2.0 * style.action.padding as f32;
    let controls = SLIDER_HEIGHT
        .max(line(&percent_label(c), BODY_FONT))
        .max(button);
    (title + style.spacing as f32 + controls + 2.0 * inset(style) as f32).ceil() as u32
}

pub fn volume_view<'a>(c: VolumeContent<'a>, style: &'a Style) -> Element<'a, Event> {
    let label = |content: String, font| {
        text(content)
            .font(font)
            .size(style.font_size)
            .line_height(LineHeight::default())
            .shaping(SHAPING)
            .wrapping(Wrapping::None)
    };
    let progress = style.progress.clone();
    let volume_slider = slider(0.0..=c.max as f32, c.percent as f32, Event::Slide)
        .on_release(Event::Release)
        .step(1.0_f32)
        .height(SLIDER_HEIGHT)
        .width(Length::Fill)
        .style(move |_, _| slider::Style {
            rail: slider::Rail {
                backgrounds: (progress.color.into(), progress.background.into()),
                width: progress.height as f32,
                border: iced::Border::default().rounded(progress.height as f32 / 2.0),
            },
            handle: slider::Handle {
                shape: slider::HandleShape::Circle { radius: 7.0 },
                background: progress.color.into(),
                border_width: 0.0,
                border_color: iced::Color::TRANSPARENT,
            },
        });
    let action = style.action.clone();
    let mute = button(
        label(mute_label(&c).into(), BODY_FONT)
            .align_x(Horizontal::Center)
            .width(Length::Fill),
    )
    .width(mute_width(style))
    .padding(action.padding as f32)
    .on_press(Event::ToggleMute)
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
    });
    let controls = row![
        volume_slider,
        label(percent_label(&c), BODY_FONT)
            .width(PERCENT_WIDTH)
            .align_x(Horizontal::Right),
        mute,
    ]
    .spacing(style.spacing)
    .align_y(Vertical::Center);
    let content = column![
        label(c.title.to_owned(), SUMMARY_FONT).width(Length::Fill),
        controls
    ]
    .spacing(style.spacing);

    mouse_area(frame(content.into(), style, inset(style)))
        .on_scroll(|delta| {
            Event::Scroll(match delta {
                mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. } => y,
            })
        })
        .on_enter(Event::Hover(true))
        .on_exit(Event::Hover(false))
        .into()
}
