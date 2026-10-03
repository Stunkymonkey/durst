//! The volume OSD: icon, device name, slider, percent, mute button.
//!
//! Like notifications it is a layer surface whose height is predicted, so
//! `volume_height` mirrors `volume_view`.

use iced::advanced::text::{LineHeight, Wrapping};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, column, container, image, mouse_area, row, slider, text};
use iced::{Element, Length, mouse};

use super::icons::Icon;
use super::notification::{
    SHAPING, body_font, frame, icon_view, inset, line_width, summary_font, text_height,
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
    /// from the icon theme, see [`volume_icon_name`]
    pub icon: Option<&'a Icon>,
}

/// The icon theme's name for a volume level (freedesktop naming spec).
pub fn volume_icon_name(mic: bool, percent: u32, muted: bool) -> &'static str {
    let level = match percent {
        _ if muted => 0,
        0 => 0,
        1..=33 => 1,
        34..=66 => 2,
        _ => 3,
    };
    let names = if mic {
        [
            "microphone-sensitivity-muted",
            "microphone-sensitivity-low",
            "microphone-sensitivity-medium",
            "microphone-sensitivity-high",
        ]
    } else {
        [
            "audio-volume-muted",
            "audio-volume-low",
            "audio-volume-medium",
            "audio-volume-high",
        ]
    };
    names[level]
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
        .map(|l| line_width(l, body_font(style), style))
        .into_iter()
        .fold(0.0, f32::max);
    (widest + 2.0 * style.action.padding as f32).ceil()
}

/// Height in pixels of the surface `volume_view` renders.
pub fn volume_height(c: &VolumeContent, style: &Style) -> u32 {
    let line = |s: &str, font| text_height(s, font, style, f32::INFINITY);
    let title = line(c.title, summary_font(style));
    let button = line(mute_label(c), body_font(style)) + 2.0 * style.action.padding as f32;
    let controls = SLIDER_HEIGHT
        .max(line(&percent_label(c), body_font(style)))
        .max(button);
    let text = title + style.spacing as f32 + controls;
    let icon = if c.icon.is_some() {
        style.icon_size as f32
    } else {
        0.0
    };
    (text.max(icon) + 2.0 * inset(style) as f32).ceil() as u32
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
        label(mute_label(&c).into(), body_font(style))
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
        label(percent_label(&c), body_font(style))
            .width(PERCENT_WIDTH)
            .align_x(Horizontal::Right),
        mute,
    ]
    .spacing(style.spacing)
    .align_y(Vertical::Center);
    let texts = column![
        label(c.title.to_owned(), summary_font(style)).width(Length::Fill),
        controls
    ]
    .spacing(style.spacing);
    let content = match c.icon {
        Some(icon) => row![icon_view(icon, style.icon_size), texts]
            .spacing(style.spacing)
            .align_y(Vertical::Center)
            .into(),
        None => Element::from(texts),
    };

    mouse_area(frame(content, style, inset(style)))
        .on_scroll(|delta| {
            Event::Scroll(match delta {
                mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. } => y,
            })
        })
        .on_enter(Event::Hover(true))
        .on_exit(Event::Hover(false))
        .into()
}

#[derive(Debug, Clone)]
pub enum MediaEvent {
    Previous,
    PlayPause,
    Next,
    Hover(bool),
}

pub struct MediaContent<'a> {
    pub title: &'a str,
    pub artist: &'a str,
    pub playing: bool,
    /// `None`: no cover, or not loaded yet; a placeholder keeps the place
    pub cover: Option<&'a image::Handle>,
    pub cover_size: u32,
}

const MEDIA_LABELS: [&str; 4] = ["Previous", "Play", "Pause", "Next"];

/// One width for all buttons, so Play/Pause doesn't move anything.
fn media_button_width(style: &Style) -> f32 {
    let widest = MEDIA_LABELS
        .map(|l| line_width(l, body_font(style), style))
        .into_iter()
        .fold(0.0, f32::max);
    (widest + 2.0 * style.action.padding as f32).ceil()
}

/// Height in pixels of the surface `media_view` renders.
pub fn media_height(c: &MediaContent, style: &Style) -> u32 {
    let line = |s: &str, font| text_height(s, font, style, f32::INFINITY);
    let spacing = style.spacing as f32;
    let mut info = line(c.title, summary_font(style));
    if !c.artist.is_empty() {
        info += spacing + line(c.artist, body_font(style));
    }
    info += spacing + line("Play", body_font(style)) + 2.0 * style.action.padding as f32;
    (info.max(c.cover_size as f32) + 2.0 * inset(style) as f32).ceil() as u32
}

pub fn media_view<'a>(c: MediaContent<'a>, style: &'a Style) -> Element<'a, MediaEvent> {
    let label = |content: &str, font| {
        text(content.to_owned())
            .font(font)
            .size(style.font_size)
            .line_height(LineHeight::default())
            .shaping(SHAPING)
            .wrapping(Wrapping::None)
    };
    let size = Length::Fixed(c.cover_size as f32);
    let cover: Element<'a, MediaEvent> = match c.cover {
        Some(handle) => image(handle.clone()).width(size).height(size).into(),
        None => {
            let background = style.progress.background;
            container(text(""))
                .width(size)
                .height(size)
                .style(move |_| container::Style {
                    background: Some(background.into()),
                    border: iced::Border::default().rounded(6.0),
                    ..Default::default()
                })
                .into()
        }
    };
    let width = media_button_width(style);
    let action = style.action.clone();
    let media_button = |content: &'static str, event: MediaEvent| {
        let action = action.clone();
        button(
            label(content, body_font(style))
                .align_x(Horizontal::Center)
                .width(Length::Fill),
        )
        .width(width)
        .padding(action.padding as f32)
        .on_press(event)
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
    };
    let buttons = row![
        media_button("Previous", MediaEvent::Previous),
        media_button(
            if c.playing { "Pause" } else { "Play" },
            MediaEvent::PlayPause
        ),
        media_button("Next", MediaEvent::Next),
    ]
    .spacing(style.spacing);
    let mut info =
        column![label(c.title, summary_font(style)).width(Length::Fill)].spacing(style.spacing);
    if !c.artist.is_empty() {
        info = info.push(label(c.artist, body_font(style)).width(Length::Fill));
    }
    info = info.push(buttons);
    // long titles are cut off instead of widening the surface
    let info = container(info).width(Length::Fill).clip(true);

    mouse_area(frame(
        row![cover, info].spacing(style.spacing).into(),
        style,
        inset(style),
    ))
    .on_enter(MediaEvent::Hover(true))
    .on_exit(MediaEvent::Hover(false))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_icon_levels() {
        let speaker = |p, m| volume_icon_name(false, p, m);
        assert_eq!(speaker(0, false), "audio-volume-muted");
        assert_eq!(speaker(80, true), "audio-volume-muted");
        assert_eq!(speaker(33, false), "audio-volume-low");
        assert_eq!(speaker(34, false), "audio-volume-medium");
        assert_eq!(speaker(67, false), "audio-volume-high");
        assert_eq!(speaker(150, false), "audio-volume-high");
        assert_eq!(
            volume_icon_name(true, 50, false),
            "microphone-sensitivity-medium"
        );
    }
}
