//! Login prompt

use iced::{text_input, Button, Column, Container, Element, Radio, Row, Text, TextInput};

use crate::ui::Message;

/// View for the login prompt
#[derive(Debug, Clone, Default)]
pub struct PromptView {
    /// Username input field
    pub user_input: text_input::State,
    /// Password input field
    pub password_input: text_input::State,
    /// Homeserver input field
    pub server_input: text_input::State,
    /// Device name input field
    pub device_input: text_input::State,
    /// Button to trigger login
    pub login_button: iced::button::State,

    /// Username
    pub user: String,
    /// Password
    pub password: String,
    /// Homeserver
    pub server: String,
    /// Device name to create login session under
    pub device_name: String,
    /// Whether to log in or sign up
    pub action: PromptAction,
    /// Error message
    pub error: Option<String>,
}

impl PromptView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn view(&mut self) -> Element<Message> {
        let mut content = Column::new()
            .width(500.into())
            .spacing(5)
            .push(
                Row::new()
                    .spacing(15)
                    .push(Radio::new(
                        PromptAction::Login,
                        "Login",
                        Some(self.action),
                        Message::SetAction,
                    ))
                    .push(Radio::new(
                        PromptAction::Signup,
                        "Sign up",
                        Some(self.action),
                        Message::SetAction,
                    )),
            )
            .push(
                Column::new().push(Text::new("Username")).push(
                    TextInput::new(
                        &mut self.user_input,
                        "Username",
                        &self.user,
                        Message::SetUser,
                    )
                    .padding(5),
                ),
            )
            .push(
                Column::new().push(Text::new("Password")).push(
                    TextInput::new(
                        &mut self.password_input,
                        "Password",
                        &self.password,
                        Message::SetPassword,
                    )
                    .password()
                    .padding(5),
                ),
            )
            .push(
                Column::new().push(Text::new("Homeserver")).push(
                    TextInput::new(
                        &mut self.server_input,
                        "https://homeserver.com",
                        &self.server,
                        Message::SetServer,
                    )
                    .padding(5),
                ),
            )
            .push(
                Column::new().push(Text::new("Device name")).push(
                    TextInput::new(
                        &mut self.device_input,
                        "retrix on my laptop",
                        &self.device_name,
                        Message::SetDeviceName,
                    )
                    .padding(5),
                ),
            );
        let button = match self.action {
            PromptAction::Login => {
                Button::new(&mut self.login_button, Text::new("Login")).on_press(Message::Login)
            }
            PromptAction::Signup => {
                content = content.push(
                    Text::new("NB: Signup is very naively implemented, and prone to breaking")
                        .color([1.0, 0.5, 0.0]),
                );
                Button::new(&mut self.login_button, Text::new("Sign up")).on_press(Message::Signup)
            }
        };
        content = content.push(button);
        if let Some(ref error) = self.error {
            content = content.push(Text::new(error).color([1.0, 0.0, 0.0]));
        }

        Container::new(content)
            .center_x()
            .center_y()
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptAction {
    Login,
    Signup,
}

impl Default for PromptAction {
    fn default() -> Self {
        PromptAction::Login
    }
}
