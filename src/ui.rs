use iced::{
    text_input::{self, TextInput},
    Application, Button, Column, Command, Container, Element, Length, Row, Text,
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
    LoggedIn {
        client: matrix_sdk::Client,
        session: matrix_sdk::Session,

        rooms: Vec<matrix_sdk::Room>,
    },
}

#[derive(Debug, Clone)]
pub enum Message {
    // Login form messages
    SetUser(String),
    SetPassword(String),
    SetServer(String),
    Login,
    LoggedIn(matrix_sdk::Client, matrix_sdk::Session),
    SetError(String),

    // Main state messages
    ResetRooms(Vec<matrix_sdk::Room>),
}

impl Application for Retrix {
    type Message = Message;
    type Executor = iced::executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Self::Message>) {
        let app = Retrix::Prompt {
            user_input: text_input::State::new(),
            password_input: text_input::State::new(),
            server_input: text_input::State::new(),
            login_button: Default::default(),

            user: String::new(),
            password: String::new(),
            server: String::new(),
            error: None,
        };
        (app, Command::none())
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
                ref mut error,
                ..
            } => match message {
                Message::SetUser(u) => *user = u,
                Message::SetPassword(p) => *password = p,
                Message::SetServer(s) => *server = s,
                Message::SetError(e) => *error = Some(e),
                Message::Login => {
                    let user = user.clone();
                    let password = password.clone();
                    let server = server.clone();
                    return Command::perform(
                        async move { matrix::login(&user, &password, &server).await },
                        |result| match result {
                            Ok((c, r)) => Message::LoggedIn(c, r),
                            Err(e) => Message::SetError(e.to_string()),
                        },
                    );
                }
                Message::LoggedIn(client, session) => {
                    *self = Retrix::LoggedIn {
                        client: client.clone(),
                        session,
                        rooms: Vec::new(),
                    };
                    let client = client.clone();
                    Command::perform(
                        async move {
                            let mut list = Vec::new();
                            for (id, room) in client.joined_rooms().read().await.iter() {
                                let room = room.read().await;
                                list.push(room.clone());
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
            Retrix::LoggedIn { ref rooms, .. } => {
                //let mut root_row = Row::new().width(Length::Fill).height(Length::Fill);
                let mut room_col = Column::new().width(400.into()).height(Length::Fill);
                for room in rooms {
                    room_col = room_col.push(Text::new(room.display_name()));
                }
                room_col.into()
                //root_row = root_row.push(room_col);
                //root_row.into()
            }
        }
    }
}
