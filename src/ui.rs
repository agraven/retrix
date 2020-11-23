use iced::{
    text_input::{self, TextInput},
    Column, Container, Element, Row, Sandbox, Settings, Text,
};

#[derive(Debug, Clone)]
pub enum Retrix {
    Prompt {
        user_input: text_input::State,
        user: String,
        password_input: text_input::State,
        password: String,
        server_input: text_input::State,
        server: String,
    },
    LoggedIn,
}

#[derive(Debug, Clone)]
pub enum Message {
    SetUser(String),
    SetPassword(String),
    SetServer(String),
}

impl Sandbox for Retrix {
    type Message = Message;

    fn new() -> Self {
        Retrix::Prompt {
            user_input: text_input::State::new(),
            user: String::new(),
            password_input: text_input::State::new(),
            password: String::new(),
            server_input: text_input::State::new(),
            server: String::new(),
        }
    }

    fn title(&self) -> String {
        String::from("Retrix matrix client")
    }

    fn update(&mut self, message: Self::Message) {}

    fn view(&mut self) -> Element<Self::Message> {
        match *self {
            Retrix::Prompt {
                ref mut user_input,
                ref user,
                ref mut password_input,
                ref password,
                ref mut server_input,
                ref server,
            } => {
                let content = Column::new()
                    .push(Text::new("Username"))
                    .push(TextInput::new(user_input, "Username", user, |val| {
                        Message::SetUser(val)
                    }))
                    .push(Text::new("Password"))
                    .push(TextInput::new(
                        password_input,
                        "Password",
                        password,
                        |val| Message::SetPassword(val),
                    ))
                    .push(Text::new("Homeserver"))
                    .push(TextInput::new(server_input, "Server", server, |val| {
                        Message::SetServer(val)
                    }));
                content.into()
            }
            _ => Text::new("Beep").into(),
        }
    }
}
