use iced::{
    text_input::{self, TextInput},
    Button, Column, Container, Element, Sandbox, Text,
};

#[derive(Debug, Clone)]
pub enum Retrix {
    Prompt {
        user_input: text_input::State,
        password_input: text_input::State,
        server_input: text_input::State,
        login_button: iced::button::State,

        user: String,
        password: String,
        server: String,
    },
    LoggedIn,
}

#[derive(Debug, Clone)]
pub enum Message {
    SetUser(String),
    SetPassword(String),
    SetServer(String),
    Login,
}

impl Sandbox for Retrix {
    type Message = Message;

    fn new() -> Self {
        Retrix::Prompt {
            user_input: text_input::State::new(),
            password_input: text_input::State::new(),
            server_input: text_input::State::new(),
            login_button: Default::default(),

            user: String::new(),
            password: String::new(),
            server: String::new(),
        }
    }

    fn title(&self) -> String {
        String::from("Retrix matrix client")
    }

    fn update(&mut self, message: Self::Message) {
        match *self {
            Retrix::Prompt {
                ref mut user,
                ref mut password,
                ref mut server,
                ..
            } => match message {
                Message::SetUser(u) => *user = u,
                Message::SetPassword(p) => *password = p,
                Message::SetServer(s) => *server = s,
                Message::Login => (),
            },
            _ => (),
        }
    }

    fn view(&mut self) -> Element<Self::Message> {
        match *self {
            Retrix::Prompt {
                ref mut user_input,
                ref mut password_input,
                ref mut server_input,
                ref mut login_button,
                ref user,
                ref password,
                ref server,
            } => {
                let content = Column::new()
                    .width(500.into())
                    .push(Text::new("Username"))
                    .push(
                        TextInput::new(user_input, "Username", user, |val| Message::SetUser(val))
                            .padding(5),
                    )
                    .push(Text::new("Password"))
                    .push(
                        TextInput::new(password_input, "Password", password, |val| {
                            Message::SetPassword(val)
                        })
                        .password()
                        .padding(5),
                    )
                    .push(Text::new("Homeserver"))
                    .push(
                        TextInput::new(server_input, "Server", server, |val| {
                            Message::SetServer(val)
                        })
                        .padding(5),
                    )
                    .push(Button::new(login_button, Text::new("Login")).on_press(Message::Login));

                Container::new(content)
                    .center_x()
                    .center_y()
                    .width(iced::Length::Fill)
                    .height(iced::Length::Fill)
                    .into()
            }
            _ => Text::new("Beep").into(),
        }
    }
}
