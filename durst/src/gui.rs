use iced::{Alignment, Border, Color, Command, Element, Length, Theme};
use iced::application::StyleSheet;
use iced::widget::{button, column, row, container, svg, text};
use iced_layershell::Application;
use iced_layershell::reexport::{Anchor, KeyboardInteractivity};
use iced_layershell::settings::{LayerShellSettings, Settings};
use iced_style::application;

#[derive(Debug, Default)]
pub struct UINotification {
    value: i32,
    text: String,
    icon: String,
}

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

    fn new(flags: Flags) -> (Self, Command<Message>) {
        (
            Self {
                value: 0,
                text: flags.app_name,
                icon: flags.app_icon,
                ..Default::default()
            },
            Command::none(),
        )
    }

    fn namespace(&self) -> String {
        String::from("durst")
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::IncrementPressed => {
                self.value += 1;
                Command::none()
            }
            Message::DecrementPressed => {
                self.value -= 1;
                Command::none()
            }
        }
    }

    fn theme(&self) -> Self::Theme {
        iced::Theme::CatppuccinMocha
    }

    fn style(&self) -> <Self::Theme as StyleSheet>::Style {
        <Self::Theme as iced::application::StyleSheet>::Style::Custom(Box::new(CustomTheme))
    }

    fn view(&self) -> Element<Message> {
        let counter_stuff = column![
            button("Increment").on_press(Message::IncrementPressed),
            text(self.value).size(50),
            button("Decrement").on_press(Message::DecrementPressed)
        ]
        .padding(20)
        .align_items(Alignment::Center);

        let handle = svg::Handle::from_path(format!(
            "{}/resources/test.svg",
            env!("CARGO_MANIFEST_DIR")
        ));

        // in iced 0.13: https://docs.iced.rs/iced/widget/struct.Svg.html
        // svg.opacity(0.7)
        let svg = svg(handle).width(Length::Fill).height(Length::Fill).content_fit(iced::ContentFit::Cover);

        container(
            column![
                row![
                    svg,
                    text(self.text.clone()).size(20),
                    counter_stuff,
                ]
                .height(Length::Fill)
                .spacing(10)
                .align_items(Alignment::Center)
                .padding(10),
            ].padding(10),
        )
        .style(<iced_style::Theme as container::StyleSheet>::Style::Custom(
            Box::new(CustomTheme),
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}

// themeing

pub struct CustomTheme;

impl container::StyleSheet for CustomTheme {
    type Style = iced::Theme;

    fn appearance(&self, _style: &Self::Style) -> container::Appearance {
        container::Appearance {
            border: Border {
                color: Color::from_rgba(0.5, 0.5, 1.0, 0.4),
                width: 40.0,
                radius: 30.0.into(),
            },
            background: Some(Color::from_rgba(0.0, 1.0, 0.0, 0.6).into()),
            ..container::Appearance::default()
        }
    }
}

impl iced_style::application::StyleSheet for CustomTheme {
    type Style = iced::Theme;

    fn appearance(&self, _style: &Self::Style) -> application::Appearance {
        iced_style::application::Appearance {
            background_color: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
            text_color: Color::from_rgba(1.0, 0.5, 0.5, 0.9),
        }
    }
}
