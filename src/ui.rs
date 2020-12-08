use std::collections::{BTreeMap, HashMap};

use futures::executor::block_on;
use iced::{
    text_input::{self, TextInput},
    Application, Button, Column, Command, Container, Element, Length, Radio, Row, Rule, Scrollable,
    Subscription, Text,
};
use matrix_sdk::{
    events::{
        key::verification::cancel::CancelCode as VerificationCancelCode,
        room::message::MessageEventContent, AnyMessageEventContent,
        AnyPossiblyRedactedSyncMessageEvent, AnySyncMessageEvent, AnyToDeviceEvent,
    },
    identifiers::RoomId,
};

use crate::matrix;

/// View for the login prompt
#[derive(Debug, Clone, Default)]
pub struct PromptView {
    /// Username input field
    user_input: text_input::State,
    /// Password input field
    password_input: text_input::State,
    /// Homeserver input field
    server_input: text_input::State,
    /// Device name input field
    device_input: text_input::State,
    /// Button to trigger login
    login_button: iced::button::State,

    /// Username
    user: String,
    /// Password
    password: String,
    /// Homeserver
    server: String,
    /// Device name to create login session under
    device_name: String,
    /// Whether to log in or sign up
    action: PromptAction,
    /// Error message
    error: Option<String>,
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
pub enum RoomSorting {
    Recent,
    Alphabetic,
}

/// Main view after successful login
#[derive(Debug, Clone)]
pub struct MainView {
    /// Settings view, if open
    settings_view: Option<SettingsView>,
    /// The matrix-sdk client
    client: matrix_sdk::Client,
    session: matrix::Session,
    /// Draft of message to send
    draft: String,
    /// Potential error message
    error: Option<(String, iced::button::State)>,
    /// Selected room
    selected: Option<RoomId>,
    /// Potential verification flow
    sas: Option<matrix_sdk::Sas>,
    /// Whether to sort rooms alphabetically or by activity
    sorting: RoomSorting,
    rooms: BTreeMap<RoomId, String>,
    messages: BTreeMap<RoomId, MessageEventContent>,

    /// Room list entry/button to select room
    buttons: HashMap<RoomId, iced::button::State>,
    /// Room list scrollbar state
    room_scroll: iced::scrollable::State,
    /// Message view scrollbar state
    message_scroll: iced::scrollable::State,
    /// Message draft text input
    message_input: iced::text_input::State,
    /// Button to send drafted message
    send_button: iced::button::State,
    /// Button to open settings menu
    settings_button: iced::button::State,
    /// Button for accepting/continuing verification
    sas_accept_button: iced::button::State,
    /// Button for cancelling verification
    sas_deny_button: iced::button::State,
}

impl MainView {
    pub fn new(client: matrix_sdk::Client, session: matrix::Session) -> Self {
        Self {
            client,
            session,
            settings_view: None,
            settings_button: Default::default(),
            error: None,
            sas: None,
            rooms: Default::default(),
            selected: None,
            room_scroll: Default::default(),
            message_scroll: Default::default(),
            message_input: Default::default(),
            buttons: Default::default(),
            messages: Default::default(),
            draft: String::new(),
            send_button: Default::default(),
            sorting: RoomSorting::Recent,
            sas_accept_button: Default::default(),
            sas_deny_button: Default::default(),
        }
    }

    pub fn view(&mut self) -> Element<Message> {
        // If settings view is open, display that instead
        if let Some(ref mut settings) = self.settings_view {
            return settings.view(self.sorting);
        }
        let mut root_row = Row::new().width(Length::Fill).height(Length::Fill);

        // Room list
        let joined = self.client.joined_rooms();
        let rooms = futures::executor::block_on(async { joined.read().await });
        let mut room_scroll = Scrollable::new(&mut self.room_scroll)
            .width(300.into())
            .height(Length::Fill)
            .scrollbar_width(5);
        // We have to iterate the buttons map and not the other way around to make the
        // borrow checker happy. First we make sure there's a button entry for every room
        // entry, and clean up button entries from removed rooms.
        for (id, _) in rooms.iter() {
            self.buttons.entry(id.to_owned()).or_default();
        }
        self.buttons.retain(|id, _| rooms.contains_key(id));
        // Then we make our buttons
        let mut buttons: HashMap<RoomId, Button<Message>> = self
            .buttons
            .iter_mut()
            .map(|(id, state)| {
                // Get read lock for the room
                let room = block_on(async { rooms.get(id).unwrap().read().await });
                let button = Button::new(state, Text::new(room.display_name()))
                    .on_press(Message::SelectRoom(id.to_owned()))
                    .width(400.into());
                (id.clone(), button)
            })
            .collect();
        // List of direct message room ids
        let mut dm_rooms: Vec<&RoomId> = rooms
            .iter()
            .filter(|(_, room)| {
                let read = block_on(async { room.read().await });
                read.direct_target.is_some()
            })
            .map(|(id, _)| id)
            .collect();
        // List of non-DM room ids
        let mut room_rooms = rooms
            .iter()
            .filter(|(_, room)| {
                let read = block_on(async { room.read().await });
                read.direct_target.is_none()
            })
            .map(|(id, _)| id)
            .collect();
        for list in [&mut dm_rooms, &mut room_rooms].iter_mut() {
            match self.sorting {
                RoomSorting::Recent => list.sort_by_key(|id| {
                    let read = block_on(async { rooms.get(id).unwrap().read().await });
                    let time = read
                        .messages
                        .iter()
                        .map(|msg| match msg {
                            AnyPossiblyRedactedSyncMessageEvent::Regular(m) => m.origin_server_ts(),
                            AnyPossiblyRedactedSyncMessageEvent::Redacted(m) => {
                                m.origin_server_ts()
                            }
                        })
                        .max()
                        .copied();
                    match time {
                        Some(time) => time,
                        None => {
                            println!("couldn't get time");
                            std::time::SystemTime::now()
                        }
                    }
                }),
                RoomSorting::Alphabetic => list.sort_by_cached_key(|id| {
                    let read = block_on(async { rooms.get(id).unwrap().read().await });
                    read.display_name().to_uppercase()
                }),
            };
        }

        // Add buttons to room column
        room_scroll = room_scroll.push(Text::new("Direct messages"));
        for button in dm_rooms.iter().map(|id| buttons.remove(id).unwrap()) {
            room_scroll = room_scroll.push(button);
        }
        room_scroll = room_scroll.push(Text::new("Rooms"));
        for button in room_rooms.iter().map(|id| buttons.remove(id).unwrap()) {
            room_scroll = room_scroll.push(button);
        }

        let room_col = Column::new()
            .push(
                Button::new(&mut self.settings_button, Text::new("Settings"))
                    .on_press(Message::OpenSettings),
            )
            .push(room_scroll);

        root_row = root_row.push(room_col);

        // Messages.
        //
        // Get selected room.
        let mut message_col = Column::new().spacing(5).padding(5);
        let selected_room = self.selected.as_ref().and_then(|selected| {
            futures::executor::block_on(async {
                match rooms.get(selected) {
                    Some(room) => Some(room.read().await),
                    None => None,
                }
            })
        });
        if let Some(room) = selected_room {
            message_col = message_col
                .push(Text::new(room.display_name()).size(25))
                .push(Rule::horizontal(2));
            let mut scroll = Scrollable::new(&mut self.message_scroll)
                .scrollbar_width(2)
                .height(Length::Fill);
            for message in room.messages.iter() {
                if let AnyPossiblyRedactedSyncMessageEvent::Regular(event) = message {
                    if let AnySyncMessageEvent::RoomMessage(room_message) = event {
                        match &room_message.content {
                            MessageEventContent::Text(text) => {
                                // Render senders disambiguated name or fallback to mxid
                                let sender = Text::new(
                                    room.joined_members
                                        .get(&room_message.sender)
                                        .map(|sender| sender.disambiguated_name())
                                        .unwrap_or(room_message.sender.to_string()),
                                )
                                .color([0.2, 0.2, 1.0]);
                                let row = Row::new()
                                    .spacing(5)
                                    .push(sender)
                                    .push(Text::new(&text.body).width(Length::Fill))
                                    .push(Text::new(format_systime(room_message.origin_server_ts)));
                                scroll = scroll.push(row);
                            }
                            _ => (),
                        }
                    }
                }
            }
            message_col = message_col.push(scroll);
        } else {
            message_col = message_col.push(
                Container::new(Text::new("Select a room to start chatting"))
                    .center_x()
                    .center_y()
                    .width(Length::Fill)
                    .height(Length::Fill),
            );
        }
        // Verification info
        if let Some(ref sas) = self.sas {
            let device = sas.other_device();
            let sas_row = match sas.emoji() {
                Some(emojis) => {
                    let mut row = Row::new().push(Text::new("Verify emojis match:"));
                    for (emoji, name) in emojis.iter() {
                        row = row.push(
                            Column::new()
                                .align_items(iced::Align::Center)
                                .push(Text::new(*emoji).size(32))
                                .push(Text::new(*name)),
                        );
                    }
                    row.push(
                        Button::new(&mut self.sas_accept_button, Text::new("Confirm"))
                            .on_press(Message::VerificationConfirm),
                    )
                    .push(
                        Button::new(&mut self.sas_deny_button, Text::new("Deny"))
                            .on_press(Message::VerificationCancel),
                    )
                }
                None => Row::new()
                    .push(
                        Text::new(format!(
                            "Incoming verification request from {}",
                            match device.display_name() {
                                Some(name) => name,
                                None => device.device_id().as_str(),
                            }
                        ))
                        .width(Length::Fill),
                    )
                    .push(
                        Button::new(&mut self.sas_accept_button, Text::new("Accept"))
                            .on_press(Message::VerificationAccept),
                    )
                    .push(
                        Button::new(&mut self.sas_deny_button, Text::new("Cancel"))
                            .on_press(Message::VerificationCancel),
                    ),
            };
            message_col = message_col.push(sas_row);
        }
        // Potential error message
        if let Some((ref error, ref mut button)) = self.error {
            message_col = message_col.push(
                Row::new()
                    .push(Text::new(error).width(Length::Fill).color([1.0, 0.0, 0.0]))
                    .push(Button::new(button, Text::new("Close")).on_press(Message::ClearError)),
            );
        }
        // Compose box
        message_col = message_col.push(
            Row::new()
                .push(
                    TextInput::new(
                        &mut self.message_input,
                        "Write a message...",
                        &self.draft,
                        Message::SetMessage,
                    )
                    .width(Length::Fill)
                    .padding(5)
                    .on_submit(Message::SendMessage),
                )
                .push(
                    Button::new(&mut self.send_button, Text::new("Send"))
                        .on_press(Message::SendMessage),
                ),
        );
        root_row = root_row.push(message_col);

        root_row.into()
    }
}

#[derive(Debug, Clone)]
pub enum Retrix {
    Prompt(PromptView),
    AwaitLogin,
    LoggedIn(MainView),
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

#[derive(Debug, Clone)]
pub enum Message {
    // Login form messages
    SetUser(String),
    SetPassword(String),
    SetServer(String),
    SetDeviceName(String),
    SetAction(PromptAction),
    Login,
    Signup,
    // Auth result messages
    LoggedIn(matrix_sdk::Client, matrix::Session),
    LoginFailed(String),

    // Main state messages
    ResetRooms(BTreeMap<RoomId, String>),
    SelectRoom(RoomId),
    /// Set error message
    ErrorMessage(String),
    /// Close error message
    ClearError,
    /// Set how the room list is sorted
    SetSort(RoomSorting),
    SetVerification(Option<matrix_sdk::Sas>),
    /// Accept verification flow
    VerificationAccept,
    /// Accept sent
    VerificationAccepted,
    /// Confirm keys match
    VerificationConfirm,
    /// Confirmation sent
    VerificationConfirmed,
    /// Cancel verification flow
    VerificationCancel,
    /// Verification flow cancelled
    VerificationCancelled(VerificationCancelCode),
    /// Matrix event received
    Sync(matrix::Event),
    /// Set contents of message compose box
    SetMessage(String),
    /// Send the contents of the compose box to the selected room
    SendMessage,

    // Settings messages
    /// Open settings menu
    OpenSettings,
    /// Close settings menu
    CloseSettings,
    /// Set display name input field
    SetDisplayNameInput(String),
    /// Save new display name
    SaveDisplayName,
    /// New display name saved successfully
    DisplayNameSaved,
    /// Set key import path
    SetKeyPath(String),
    /// Set password key backup is encrypted with
    SetKeyPassword(String),
    /// Import encryption keys
    ImportKeys,
}

#[derive(Clone, Default, Debug)]
pub struct SettingsView {
    /// Display name to set
    display_name: String,
    /// Are we saving the display name?
    saving_name: bool,

    /// Display name text input
    display_name_input: iced::text_input::State,
    /// Button to set display name
    display_name_button: iced::button::State,

    /// Path to import encryption keys from
    key_path: String,
    /// Password to decrypt the keys with
    key_password: String,

    /// Encryption key path entry
    key_path_input: iced::text_input::State,
    /// Entry for key password
    key_password_input: iced::text_input::State,
    /// Button to import keys
    key_import_button: iced::button::State,
    /// Button  to close settings view
    close_button: iced::button::State,
}

impl SettingsView {
    fn new() -> Self {
        Self::default()
    }

    fn view(&mut self, sort: RoomSorting) -> Element<Message> {
        let content = Column::new()
            .width(500.into())
            .spacing(5)
            .push(Text::new("Profile").size(25))
            .push(
                Column::new().push(Text::new("Display name")).push(
                    Row::new()
                        .push(
                            TextInput::new(
                                &mut self.display_name_input,
                                "Alice",
                                &self.display_name,
                                Message::SetDisplayNameInput,
                            )
                            .width(Length::Fill)
                            .padding(5),
                        )
                        .push(match self.saving_name {
                            false => Button::new(&mut self.display_name_button, Text::new("Save"))
                                .on_press(Message::SaveDisplayName),
                            true => {
                                Button::new(&mut self.display_name_button, Text::new("Saving..."))
                            }
                        }),
                ),
            )
            .push(Text::new("Appearance").size(25))
            .push(Text::new("Sort messages by:"))
            .push(Radio::new(
                RoomSorting::Alphabetic,
                "Name",
                Some(sort),
                Message::SetSort,
            ))
            .push(Radio::new(
                RoomSorting::Recent,
                "Activity",
                Some(sort),
                Message::SetSort,
            ))
            .push(Text::new("Encryption").size(25))
            .push(
                Column::new()
                    .push(Text::new("Import key (enter path)"))
                    .push(
                        TextInput::new(
                            &mut self.key_path_input,
                            "/home/user/exported_keys.txt",
                            &self.key_path,
                            Message::SetKeyPath,
                        )
                        .padding(5),
                    ),
            )
            .push(
                Column::new().push(Text::new("Key password")).push(
                    TextInput::new(
                        &mut self.key_password_input,
                        "SecretPassword42",
                        &self.key_password,
                        Message::SetKeyPassword,
                    )
                    .password()
                    .padding(5),
                ),
            )
            .push(
                Button::new(&mut self.key_import_button, Text::new("Import keys"))
                    .on_press(Message::ImportKeys),
            )
            .push(
                Row::new().width(Length::Fill).push(
                    Button::new(&mut self.close_button, Text::new("Close"))
                        .on_press(Message::CloseSettings),
                ),
            );
        Container::new(content)
            .center_x()
            .center_y()
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
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
            None => (Retrix::Prompt(PromptView::new()), Command::none()),
        }
    }

    fn title(&self) -> String {
        String::from("Retrix matrix client")
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        match self {
            Retrix::LoggedIn(view) => {
                matrix::MatrixSync::subscription(view.client.clone()).map(Message::Sync)
            }
            _ => Subscription::none(),
        }
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match self {
            Retrix::Prompt(prompt) => match message {
                Message::SetUser(u) => prompt.user = u,
                Message::SetPassword(p) => prompt.password = p,
                Message::SetServer(s) => prompt.server = s,
                Message::SetDeviceName(n) => prompt.device_name = n,
                Message::SetAction(a) => prompt.action = a,
                Message::Login => {
                    let user = prompt.user.clone();
                    let password = prompt.password.clone();
                    let server = prompt.server.clone();
                    let device = prompt.device_name.clone();
                    let device = match device.is_empty() {
                        false => Some(device),
                        true => None,
                    };
                    *self = Retrix::AwaitLogin;
                    return Command::perform(
                        async move { matrix::login(&user, &password, &server, device.as_deref()).await },
                        |result| match result {
                            Ok((c, r)) => Message::LoggedIn(c, r),
                            Err(e) => Message::LoginFailed(e.to_string()),
                        },
                    );
                }
                Message::Signup => {
                    let user = prompt.user.clone();
                    let password = prompt.password.clone();
                    let server = prompt.server.clone();
                    let device = prompt.device_name.clone();
                    let device = match device.is_empty() {
                        false => Some(device),
                        true => None,
                    };
                    *self = Retrix::AwaitLogin;
                    return Command::perform(
                        async move {
                            matrix::signup(&user, &password, &server, device.as_deref()).await
                        },
                        |result| match result {
                            Ok((client, response)) => Message::LoggedIn(client, response),
                            Err(e) => Message::LoginFailed(e.to_string()),
                        },
                    );
                }
                _ => (),
            },
            Retrix::AwaitLogin => match message {
                Message::LoginFailed(e) => {
                    let mut view = PromptView::default();
                    view.error = Some(e);
                    *self = Retrix::Prompt(view);
                }
                Message::LoggedIn(client, session) => {
                    *self = Retrix::LoggedIn(MainView::new(client, session));
                    /*let client = client.clone();
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
                    );*/
                }
                _ => (),
            },
            Retrix::LoggedIn(view) => {
                match message {
                    Message::ErrorMessage(e) => view.error = Some((e, Default::default())),
                    Message::ClearError => view.error = None,
                    Message::SetSort(s) => view.sorting = s,
                    Message::ResetRooms(r) => view.rooms = r,
                    Message::SelectRoom(r) => view.selected = Some(r),
                    Message::Sync(event) => match event {
                        matrix::Event::Room(_) => (),
                        matrix::Event::ToDevice(event) => match event {
                            AnyToDeviceEvent::KeyVerificationStart(start) => {
                                let client = view.client.clone();
                                return Command::perform(
                                    async move {
                                        tokio::time::delay_for(std::time::Duration::from_secs(2))
                                            .await;
                                        client.get_verification(&start.content.transaction_id).await
                                    },
                                    Message::SetVerification,
                                );
                            }
                            AnyToDeviceEvent::KeyVerificationCancel(cancel) => {
                                return async {
                                    Message::VerificationCancelled(cancel.content.code)
                                }
                                .into();
                            }
                            _ => (),
                        },
                    },
                    Message::SetVerification(v) => view.sas = v,
                    Message::VerificationAccept => {
                        let sas = match &view.sas {
                            Some(sas) => sas.clone(),
                            None => return Command::none(),
                        };
                        return Command::perform(async move { sas.accept().await }, |result| {
                            match result {
                                Ok(()) => Message::VerificationAccepted,
                                Err(e) => Message::ErrorMessage(e.to_string()),
                            }
                        });
                    }
                    Message::VerificationConfirm => {
                        let sas = match &view.sas {
                            Some(sas) => sas.clone(),
                            None => return Command::none(),
                        };
                        return Command::perform(async move { sas.confirm().await }, |result| {
                            match result {
                                Ok(()) => Message::VerificationConfirmed,
                                Err(e) => Message::ErrorMessage(e.to_string()),
                            }
                        });
                    }
                    Message::VerificationCancel => {
                        let sas = match &view.sas {
                            Some(sas) => sas.clone(),
                            None => return Command::none(),
                        };
                        return Command::perform(async move { sas.cancel().await }, |result| {
                            match result {
                                Ok(()) => {
                                    Message::VerificationCancelled(VerificationCancelCode::User)
                                }
                                Err(e) => Message::ErrorMessage(e.to_string()),
                            }
                        });
                    }
                    Message::VerificationCancelled(code) => {
                        view.sas = None;
                        return async move { Message::ErrorMessage(code.as_str().to_owned()) }
                            .into();
                    }
                    Message::SetMessage(m) => view.draft = m,
                    Message::SendMessage => {
                        let selected = match view.selected.clone() {
                            Some(selected) => selected,
                            None => return Command::none(),
                        };
                        let draft = view.draft.clone();
                        let client = view.client.clone();
                        return Command::perform(
                            async move {
                                client
                                    .room_send(
                                        &selected,
                                        AnyMessageEventContent::RoomMessage(
                                            MessageEventContent::text_plain(draft),
                                        ),
                                        None,
                                    )
                                    .await
                            },
                            |result| match result {
                                Ok(_) => Message::SetMessage(String::new()),
                                Err(e) => Message::ErrorMessage(e.to_string()),
                            },
                        );
                    }
                    Message::OpenSettings => {
                        view.settings_view = Some(SettingsView::new());
                        let client = view.client.clone();
                        return Command::perform(
                            async move {
                                client
                                    .display_name()
                                    .await
                                    .unwrap_or_default()
                                    .unwrap_or_default()
                            },
                            Message::SetDisplayNameInput,
                        );
                    }
                    Message::SetDisplayNameInput(name) => {
                        if let Some(ref mut settings) = view.settings_view {
                            settings.display_name = name;
                        }
                    }
                    Message::SaveDisplayName => {
                        if let Some(ref mut settings) = view.settings_view {
                            let client = view.client.clone();
                            let name = settings.display_name.clone();
                            settings.saving_name = true;
                            return Command::perform(
                                async move { client.set_display_name(Some(&name)).await },
                                |result| match result {
                                    Ok(()) => Message::DisplayNameSaved,
                                    // TODO: set saving to false and report error
                                    Err(_) => Message::DisplayNameSaved,
                                },
                            );
                        }
                    }
                    Message::DisplayNameSaved => {
                        if let Some(ref mut settings) = view.settings_view {
                            settings.saving_name = false;
                        }
                    }
                    Message::SetKeyPath(p) => {
                        if let Some(ref mut settings) = view.settings_view {
                            settings.key_path = p;
                        }
                    }
                    Message::SetKeyPassword(p) => {
                        if let Some(ref mut settings) = view.settings_view {
                            settings.key_password = p;
                        }
                    }
                    Message::ImportKeys => {
                        if let Some(ref settings) = view.settings_view {
                            let path = std::path::PathBuf::from(&settings.key_path);
                            let password = settings.key_password.clone();
                            let client = view.client.clone();
                            return Command::perform(
                                async move { client.import_keys(path, &password).await },
                                |result| match result {
                                    Ok(_) => Message::SetKeyPassword(String::new()),
                                    // TODO: Actual error reporting here
                                    Err(e) => Message::SetKeyPath(e.to_string()),
                                },
                            );
                        }
                    }
                    Message::CloseSettings => view.settings_view = None,
                    _ => (),
                }
            }
        };
        Command::none()
    }

    fn view(&mut self) -> Element<Self::Message> {
        match self {
            Retrix::Prompt(prompt) => prompt.view(),
            Retrix::AwaitLogin => Container::new(Text::new("Logging in..."))
                .center_x()
                .center_y()
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            Retrix::LoggedIn(view) => view.view(),
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
