use std::collections::{BTreeMap, HashMap};

use iced::{
    text_input::{self, TextInput},
    Application, Button, Column, Command, Container, Element, Length, Row, Rule, Scrollable,
    Subscription, Text,
};
use matrix_sdk::{
    events::{
        room::message::MessageEventContent, AnyPossiblyRedactedSyncMessageEvent, AnyRoomEvent,
        AnySyncMessageEvent,
    },
    identifiers::RoomId,
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
    AwaitLogin(std::time::Instant),
    LoggedIn {
        client: matrix_sdk::Client,
        session: matrix::Session,

        rooms: BTreeMap<RoomId, String>,
        buttons: HashMap<RoomId, iced::button::State>,
        messages: BTreeMap<RoomId, MessageEventContent>,
        selected: Option<RoomId>,
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
    ResetRooms(BTreeMap<RoomId, String>),
    SelectRoom(RoomId),
    Sync(AnyRoomEvent),
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
                (Retrix::AwaitLogin(std::time::Instant::now()), command)
            }
            None => (Retrix::new_prompt(), Command::none()),
        }
    }

    fn title(&self) -> String {
        String::from("Retrix matrix client")
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        match self {
            Retrix::LoggedIn { client, .. } => {
                matrix::MatrixSync::subscription(client.clone()).map(Message::Sync)
            }
            _ => Subscription::none(),
        }
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match self {
            Retrix::Prompt {
                user,
                password,
                server,
                ..
            } => match message {
                Message::SetUser(u) => *user = u,
                Message::SetPassword(p) => *password = p,
                Message::SetServer(s) => *server = s,
                Message::Login => {
                    let user = user.clone();
                    let password = password.clone();
                    let server = server.clone();
                    *self = Retrix::AwaitLogin(std::time::Instant::now());
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
            Retrix::AwaitLogin(_) => match message {
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
                        rooms: BTreeMap::new(),
                        selected: None,
                        room_scroll: Default::default(),
                        buttons: Default::default(),
                        messages: Default::default(),
                    };
                    let client = client.clone();
                    return Command::perform(
                        async move {
                            let mut rooms = BTreeMap::new();
                            for (id, room) in client.joined_rooms().read().await.iter() {
                                let name = room.read().await.display_name();
                                rooms.insert(id.to_owned(), name);
                            }
                            rooms
                        },
                        |rooms| Message::ResetRooms(rooms),
                    );
                }
                _ => (),
            },
            Retrix::LoggedIn {
                rooms, selected, ..
            } => match message {
                Message::ResetRooms(r) => *rooms = r,
                Message::SelectRoom(r) => *selected = Some(r),
                _ => (),
            },
        };
        Command::none()
    }

    fn view(&mut self) -> Element<Self::Message> {
        match self {
            Retrix::Prompt {
                user_input,
                password_input,
                server_input,
                login_button,
                user,
                password,
                server,
                error,
            } => {
                // Login form
                let mut content = Column::new()
                    .width(500.into())
                    .push(Text::new("Username"))
                    .push(TextInput::new(user_input, "Username", user, Message::SetUser).padding(5))
                    .push(Text::new("Password"))
                    .push(
                        TextInput::new(password_input, "Password", password, Message::SetPassword)
                            .password()
                            .padding(5),
                    )
                    .push(Text::new("Homeserver"))
                    .push(
                        TextInput::new(server_input, "Server", server, Message::SetServer)
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
            Retrix::AwaitLogin(instant) => Container::new(Text::new(format!(
                "Logging in{}",
                match instant.elapsed().subsec_millis() / 333 {
                    0 => ".",
                    1 => "..",
                    2 => "...",
                    _ => "....",
                }
            )))
            .center_x()
            .center_y()
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
            Retrix::LoggedIn {
                client,
                room_scroll,
                buttons,
                selected,
                ..
            } => {
                let mut root_row = Row::new().width(Length::Fill).height(Length::Fill);

                // Room list
                let joined = client.joined_rooms();
                let rooms = futures::executor::block_on(async { joined.read().await });
                let mut room_col = Scrollable::new(room_scroll)
                    .width(400.into())
                    .height(Length::Fill)
                    .scrollbar_width(5);
                // We have to iterate the buttons map and not the other way around to make the
                // borrow checker happy. First we make sure there's a button entry for every room
                // entry, and clean up button entries from removed rooms.
                for (id, _) in rooms.iter() {
                    buttons.entry(id.to_owned()).or_default();
                }
                buttons.retain(|id, _| rooms.contains_key(id));
                // Then we make our buttons
                let buttons: Vec<Button<_>> = buttons
                    .iter_mut()
                    .map(|(id, state)| {
                        // Get read lock for the room
                        let room = futures::executor::block_on(async {
                            rooms.get(id).unwrap().read().await
                        });
                        Button::new(state, Text::new(room.display_name()))
                            .on_press(Message::SelectRoom(id.to_owned()))
                            .width(400.into())
                    })
                    .collect();
                // Then we add them to our room column. What a mess.
                for button in buttons {
                    room_col = room_col.push(button);
                }
                root_row = root_row.push(room_col);

                // Messages.
                //
                // Get selected room.
                let selected_room = selected.as_ref().and_then(|selected| {
                    futures::executor::block_on(async {
                        match rooms.get(selected) {
                            Some(room) => Some(room.read().await),
                            None => None,
                        }
                    })
                });
                if let Some(room) = selected_room {
                    let mut col = Column::new()
                        .spacing(5)
                        .padding(5)
                        .push(Text::new(room.display_name()).size(25))
                        .push(Rule::horizontal(2));
                    for message in room.messages.iter() {
                        if let AnyPossiblyRedactedSyncMessageEvent::Regular(event) = message {
                            match event {
                                AnySyncMessageEvent::RoomMessage(room_message) => {
                                    match &room_message.content {
                                        MessageEventContent::Text(text) => {
                                            let row = Row::new()
                                                .spacing(5)
                                                .push(
                                                    Text::new(room_message.sender.localpart())
                                                        .color([0.2, 0.2, 1.0]),
                                                )
                                                .push(Text::new(&text.body).width(Length::Fill))
                                                .push(Text::new(format_systime(
                                                    room_message.origin_server_ts,
                                                )));
                                            col = col.push(row);
                                        }
                                        _ => (),
                                    }
                                }
                                _ => (),
                            }
                        }
                    }
                    root_row = root_row.push(col);
                }

                root_row.into()
            }
        }
    }
}

fn format_systime(time: std::time::SystemTime) -> String {
    let secs = time
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!(
        "{:02}:{:02}",
        (secs % (60 * 60 * 24)) / (60 * 60),
        (secs % (60 * 60)) / 60
    )
}
