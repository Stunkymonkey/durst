use iced::{Alignment, Background, Border, Color, Task, Element, Length, Theme};
use iced::widget::{button, column, row, container, svg, text};
use iced_layershell::{Appearance, Application};
use iced_layershell::to_layer_message;

#[derive(Debug, Default)]
pub struct UINotification {
    value: i32,
    text: String,
    icon: String,
}

#[to_layer_message]
#[derive(Debug, Clone)]
pub enum Message {
    IncrementPressed,
    DecrementPressed,
}

#[derive(Debug, Clone)]
pub struct Flags {
    pub app_name: String,
    pub app_icon: String,
}

impl Default for Flags {
    fn default() -> Self {
        Flags {
            app_name: "no text supplied".to_string(),
            app_icon: "no icon supplied".to_string(),
        }
    }
}

impl Application for UINotification {
    type Message = Message;
    type Flags = Flags;
    type Theme = Theme;
    type Executor = iced::executor::Default;

    fn new(flags: Flags) -> (Self, Task<Message>) {
        (
            Self {
                value: 0,
                text: flags.app_name,
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
            Message::IncrementPressed => {
                self.value += 1;
                Task::none()
            }
            Message::DecrementPressed => {
                self.value -= 1;
                Task::none()
            }
            _ => unreachable!(),
        }
    }

    fn theme(&self) -> Self::Theme {
        iced::Theme::CatppuccinMocha
    }

    fn view(&self) -> Element<Message> {
        let counter_stuff = column![
            button("Increment").on_press(Message::IncrementPressed),
            text(self.value).size(50),
            button("Decrement").on_press(Message::DecrementPressed)
        ]
        .padding(20)
        .align_x(Alignment::Center);

        let handle = svg::Handle::from_path(format!(
            "{}/resources/test.svg",
            env!("CARGO_MANIFEST_DIR")
        ));

        let svg = svg(handle)
            .width(iced::Length::Fixed(50.0))
            .height(iced::Length::Fixed(50.0))
            // .content_fit(iced::ContentFit::Cover)
            .opacity(0.7);

        container(
            column![
                row![
                    svg,
                    text(self.text.clone()).size(20),
                    counter_stuff,
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
        text_color: Some(Color::from_rgba(1.0, 0.5, 0.5, 0.9)),
        border: iced::Border {
            color: Color::from_rgba(0.5, 0.5, 1.0, 0.4),
            width: 20.0,
            radius: 30.0.into(),
        },
        shadow: iced::Shadow {
            //has to be here as empty shadow is not allowed and no paddings yet to make it visible
            color: Color::TRANSPARENT,
            offset: iced::Vector { x: 0.0, y: 0.0 },
            blur_radius: 0.0,
        },
        background: Some(iced::Background::Color(Color::from_rgba(0.0, 1.0, 0.0, 0.6))),
    }
}
