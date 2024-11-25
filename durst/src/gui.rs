use iced::{Alignment, Color, Task, Element, Length, Theme};
use iced::widget::{column, row, container, svg, text};
use iced_layershell::{MultiApplication, to_layer_message};
use std::path::PathBuf;

use crate::notification::RawNotification;

#[derive(Debug, Default)]
pub struct UINotification {
    id: u32,
    app_name: String,
    body: String,
    summary: String,
    icon: PathBuf,
}

#[to_layer_message(multi, info_name = "Flags")]
#[derive(Debug, Clone)]
pub enum Message {
    Move,
    Close,
    Notify(crate::notification::RawNotification),
}

// type WindowInfo = Flags;

#[derive(Debug, Clone)]
pub struct Flags {
    pub notification: RawNotification,
    pub app_icon: PathBuf,
}

impl Default for Flags {
    fn default() -> Self {
        Flags {
            notification: RawNotification::default(),
            app_icon: PathBuf::new(),
        }
    }
}

impl MultiApplication for UINotification {
    type Message = Message;
    type Flags = Flags;
    type Theme = Theme;
    type Executor = iced::executor::Default;
    type WindowInfo = Flags;

    fn new(flags: Flags) -> (Self, Task<Message>) {
        (
            Self {
                id: flags.notification.id,
                app_name: flags.notification.app_name,
                summary: flags.notification.summary,
                body: flags.notification.body,
                icon: flags.app_icon,
                ..Default::default()
            },
            Task::none(),
        )
    }

    fn namespace(&self) -> String {
        String::from("durst")
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Move => {
                // TODO move
                Task::none()
            }
            Message::Close => {
                // TODO close
                Task::none()
            }
            Message::Notify(_notification) => {
                // TODO notify
                Task::none()
            }
            _ => unreachable!(),
        }
    }

    fn theme(&self) -> Self::Theme {
        iced::Theme::CatppuccinMocha
    }

    // needed so transparent is activated for the whole window
    fn style(&self, theme: &Self::Theme) -> iced_layershell::Appearance {
        use iced_layershell::Appearance;
        Appearance {
            background_color: Color::TRANSPARENT,
            text_color: theme.palette().text,
        }
    }

    fn view(&self, _id: iced::window::Id) -> Element<Message> {

        let handle = svg::Handle::from_path(self.icon.clone());

        let svg = svg(handle)
            .width(iced::Length::Fixed(50.0))
            .height(iced::Length::Fixed(50.0))
            // .content_fit(iced::ContentFit::Cover)
            .opacity(0.7);

        container(
            column![
                row![
                    svg,
                    text(self.app_name.clone()).size(20),
                    text(self.body.clone()).size(20),
                    text(self.summary.clone()).size(20),
                ]
                .height(Length::Fill)
                .spacing(10)
                .align_y(Alignment::Center)
                .padding(10),
            ]
            .padding(10),
        )
        .style(move |_| iced_container_style())
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}

// themeing
fn iced_container_style() -> iced::widget::container::Style {
    // let config = crate::data::shared_data::CONFIG.lock().unwrap();
    iced::widget::container::Style {
        text_color: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.8)),
        border: iced::Border {
            color: Color::from_rgba(0.2, 0.2, 0.2, 0.4),
            width: 10.0,
            radius: 30.0.into(),
        },
        shadow: iced::Shadow {
            //has to be here as empty shadow is not allowed and no paddings yet to make it visible
            color: Color::TRANSPARENT,
            offset: iced::Vector { x: 0.0, y: 0.0 },
            blur_radius: 25.0,
        },
        background: Some(iced::Background::Color(Color::from_rgba(0.2, 0.2, 1.0, 0.6))),
    }
}
