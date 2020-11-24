use iced::{
    text_input::{self, TextInput},
    Application, Button, Column, Command, Container, Element, Length, Scrollable, Text,
};

use crate::matrix;

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
        error: Option<String>,
    },
    AwaitLogin,
    LoggedIn {
        client: matrix_sdk::Client,
        session: matrix::Session,

        rooms: Vec<String>,
        room_scroll: iced::scrollable::State,
    },
}

impl Retrix {
    pub fn new_prompt() -> Retrix {
        Retrix::Prompt {
            user_input: text_input::State::new(),
            password_input: text_input::State::new(),
            server_input: text_input::State::new(),
            login_button: Default::default(),

            user: String::new(),
            password: String::new(),
            server: String::new(),
            error: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    // Login form messages
    SetUser(String),
    SetPassword(String),
    SetServer(String),
    Login,
    LoggedIn(matrix_sdk::Client, matrix::Session),
    LoginFailed(String),

    // Main state messages
    ResetRooms(Vec<String>),
}

impl Application for Retrix {
    type Message = Message;
    type Executor = iced::executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Self::Message>) {
        // Skip login prompt if we have a session saved
        match matrix::get_session().ok().flatten() {
            Some(session) => {
                let command = Command::perform(
                    async move { matrix::restore_login(session).await },
                    |result| match result {
                        Ok((s, c)) => Message::LoggedIn(s, c),
                        Err(e) => Message::LoginFailed(e.to_string()),
                    },
                );
                (Retrix::AwaitLogin, command)
            }
            None => (Retrix::new_prompt(), Command::none()),
        }
    }

    fn title(&self) -> String {
        String::from("Retrix matrix client")
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
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
                Message::Login => {
                    let user = user.clone();
                    let password = password.clone();
                    let server = server.clone();
                    *self = Retrix::AwaitLogin;
                    return Command::perform(
                        async move { matrix::login(&user, &password, &server).await },
                        |result| match result {
                            Ok((c, r)) => Message::LoggedIn(c, r),
                            Err(e) => Message::LoginFailed(e.to_string()),
                        },
                    );
                }
                _ => (),
            },
            Retrix::AwaitLogin => match message {
                Message::LoginFailed(e) => {
                    *self = Retrix::new_prompt();
                    if let Retrix::Prompt { ref mut error, .. } = *self {
                        *error = Some(e);
                    }
                }
                Message::LoggedIn(client, session) => {
                    *self = Retrix::LoggedIn {
                        client: client.clone(),
                        session,
                        rooms: Vec::new(),
                        room_scroll: Default::default(),
                    };
                    let client = client.clone();
                    return Command::perform(
                        async move {
                            let mut list = Vec::new();
                            for (_, room) in client.joined_rooms().read().await.iter() {
                                let name = room.read().await.display_name();
                                list.push(name);
                            }
                            list
                        },
                        |rooms| Message::ResetRooms(rooms),
                    );
                }
                _ => (),
            },
            Retrix::LoggedIn { ref mut rooms, .. } => match message {
                Message::ResetRooms(r) => *rooms = r,
                _ => (),
            },
        };
        Command::none()
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
                ref error,
            } => {
                // Login form
                let mut content = Column::new()
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
                if let Some(ref error) = error {
                    content = content.push(Text::new(error).color([1.0, 0.0, 0.0]));
                }

                Container::new(content)
                    .center_x()
                    .center_y()
                    .width(iced::Length::Fill)
                    .height(iced::Length::Fill)
                    .into()
            }
            Retrix::AwaitLogin => Container::new(Text::new("Logging in..."))
                .center_x()
                .center_y()
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            Retrix::LoggedIn {
                ref rooms,
                ref mut room_scroll,
                ..
            } => {
                //let mut root_row = Row::new().width(Length::Fill).height(Length::Fill);
                let mut room_col = Scrollable::new(room_scroll)
                    .width(400.into())
                    .height(Length::Fill)
                    .spacing(15);
                for room in rooms {
                    room_col = room_col.push(Text::new(room));
                }
                room_col.into()
                //root_row = root_row.push(room_col);
                //root_row.into()
            }
        }
    }
}
