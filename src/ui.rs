use std::{
    collections::{BTreeMap, HashSet},
    time::SystemTime,
};

use futures::executor::block_on;
use iced::{
    Application, Button, Column, Command, Container, Element, Length, Row, Rule, Scrollable,
    Subscription, Text, TextInput,
};
use matrix_sdk::{
    events::{
        key::verification::cancel::CancelCode as VerificationCancelCode,
        room::message::MessageEventContent, AnyMessageEvent, AnyMessageEventContent,
        AnyPossiblyRedactedSyncMessageEvent, AnyRoomEvent, AnyStateEvent, AnyToDeviceEvent,
    },
    identifiers::{EventId, RoomAliasId, RoomId, UserId},
};

use crate::matrix::{self, AnyRoomEventExt};

pub mod prompt;
pub mod settings;

use prompt::{PromptAction, PromptView};
use settings::SettingsView;

/// What order to sort rooms in in the room list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomSorting {
    Recent,
    Alphabetic,
}

/// Data for en entry in the room list
#[derive(Clone, Debug, Default)]
pub struct RoomEntry {
    /// Cached calculated name
    name: String,
    /// Room topic
    topic: String,
    /// Canonical alias
    alias: Option<RoomAliasId>,
    /// Defined display name
    display_name: Option<String>,
    /// Person we're in a direct message with
    direct: Option<UserId>,
    /// Cache of messages
    messages: MessageBuffer,
}

impl RoomEntry {
    /// Recalculate displayname
    pub fn update_display_name(&mut self, id: &RoomId) {
        self.name = if let Some(ref name) = self.display_name {
            name.to_owned()
        } else if let Some(ref user) = self.direct {
            user.to_string()
        } else if let Some(ref alias) = self.alias {
            alias.to_string()
        } else {
            id.to_string()
        };
    }
}

// Alternate storage strategies: HashMap<EventId, Event>+Vec<EventId>,
// HashSet<EventId>+BTreemap<Event + Ord(origin_server_ts)>
/// Message history/event cache for a given room.
#[derive(Clone, Debug)]
pub struct MessageBuffer {
    /// The messages we have stored
    messages: Vec<AnyRoomEvent>,
    /// Set of event id's we have
    known_ids: HashSet<EventId>,
    /// Token for the start of the messages we have
    start: Option<String>,
    /// Token for the end of the messages we have
    end: Option<String>,
    /// Most recent activity in the room
    updated: std::time::SystemTime,
}

impl MessageBuffer {
    /// Sorts the messages by send time
    fn sort(&mut self) {
        self.messages
            .sort_unstable_by(|a, b| a.origin_server_ts().cmp(&b.origin_server_ts()))
    }

    /// Gets the send time of the most recently sent message
    fn update_time(&mut self) {
        self.updated = match self.messages.last() {
            Some(message) => message.origin_server_ts(),
            None => SystemTime::UNIX_EPOCH,
        };
    }

    /// Add a message to the buffer.
    pub fn push(&mut self, event: AnyRoomEvent) {
        self.known_ids.insert(event.event_id().clone());
        self.messages.push(event);
        self.sort();
        self.update_time();
    }

    /// Adds several messages to the buffer
    pub fn append(&mut self, mut events: Vec<AnyRoomEvent>) {
        events.retain(|e| !self.known_ids.contains(e.event_id()));
        for event in events.iter() {
            self.known_ids.insert(event.event_id().clone());
        }
        self.messages.append(&mut events);
        self.sort();
        self.update_time();
    }
}

impl Default for MessageBuffer {
    fn default() -> Self {
        Self {
            messages: Default::default(),
            known_ids: Default::default(),
            start: None,
            end: None,
            updated: SystemTime::UNIX_EPOCH,
        }
    }
}

/// Main view after successful login
#[derive(Debug, Clone)]
pub struct MainView {
    /// Settings view, if open
    settings_view: Option<SettingsView>,
    /// The matrix-sdk client
    client: matrix_sdk::Client,
    /// Sync token to use for backfill calls
    sync_token: String,
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
    /// Room state
    rooms: BTreeMap<RoomId, RoomEntry>,

    /// Room list entries for direct conversations
    dm_buttons: Vec<iced::button::State>,
    /// Room list entries for group conversations
    group_buttons: Vec<iced::button::State>,
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
            sync_token: String::new(),
            settings_view: None,
            settings_button: Default::default(),
            error: None,
            sas: None,
            rooms: Default::default(),
            selected: None,
            room_scroll: Default::default(),
            message_scroll: Default::default(),
            message_input: Default::default(),
            dm_buttons: Vec::new(),
            group_buttons: Vec::new(),
            draft: String::new(),
            send_button: Default::default(),
            sorting: RoomSorting::Alphabetic,
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

        let mut room_scroll = Scrollable::new(&mut self.room_scroll)
            .width(300.into())
            .height(Length::Fill)
            .scrollbar_width(5);

        // Group by DM and group conversation
        let (mut dm_rooms, mut group_rooms): (
            Vec<(&RoomId, &RoomEntry)>,
            Vec<(&RoomId, &RoomEntry)>,
        ) = self
            .rooms
            .iter()
            .partition(|(_, room)| room.direct.is_some());
        // Sort
        for list in [&mut dm_rooms, &mut group_rooms].iter_mut() {
            match self.sorting {
                RoomSorting::Alphabetic => list.sort_unstable_by(|(_, a), (_, b)| {
                    a.name.to_uppercase().cmp(&b.name.to_uppercase())
                }),
                RoomSorting::Recent => list.sort_unstable_by(|(_, a), (_, b)| {
                    a.messages.updated.cmp(&b.messages.updated).reverse()
                }),
            };
        }
        self.dm_buttons
            .resize_with(dm_rooms.len(), Default::default);
        self.group_buttons
            .resize_with(group_rooms.len(), Default::default);
        // Create buttons
        let dm_buttons: Vec<Button<_>> = self
            .dm_buttons
            .iter_mut()
            .enumerate()
            .map(|(idx, button)| {
                // TODO: highlight selected
                let (id, room) = dm_rooms[idx];
                Button::new(button, Text::new(&room.name))
                    .width(300.into())
                    .on_press(Message::SelectRoom(id.clone().clone()))
            })
            .collect();
        let room_buttons: Vec<Button<_>> = self
            .group_buttons
            .iter_mut()
            .enumerate()
            .map(|(idx, button)| {
                let (id, room) = group_rooms[idx];
                Button::new(button, Text::new(&room.name))
                    .width(300.into())
                    .on_press(Message::SelectRoom(id.clone()))
            })
            .collect();
        // Add buttons to container
        room_scroll = room_scroll.push(Text::new("Direct messages"));
        for button in dm_buttons.into_iter() {
            room_scroll = room_scroll.push(button);
        }
        room_scroll = room_scroll.push(Text::new("Rooms"));
        for button in room_buttons.into_iter() {
            room_scroll = room_scroll.push(button);
        }

        let room_col = Column::new()
            .push(
                Button::new(&mut self.settings_button, Text::new("Settings"))
                    .on_press(Message::OpenSettings),
            )
            .push(room_scroll);
        root_row = root_row.push(room_col);

        let mut message_col = Column::new().spacing(5).padding(5);
        let selected_room = match self.selected {
            Some(ref selected) => self.rooms.get(selected),
            None => None,
        };
        if let Some(room) = selected_room {
            message_col = message_col
                .push(Text::new(&room.name).size(25))
                .push(Rule::horizontal(2));
            let mut scroll = Scrollable::new(&mut self.message_scroll)
                .scrollbar_width(2)
                .height(Length::Fill);
            for event in room.messages.messages.iter() {
                match event {
                    AnyRoomEvent::Message(AnyMessageEvent::RoomMessage(message)) => {
                        let sender = {
                            let joined = self.client.joined_rooms();
                            let rooms_lock = block_on(async { joined.read().await });
                            match rooms_lock.get(&message.room_id) {
                                Some(backend) => {
                                    let room_lock = block_on(async { backend.read().await });
                                    match room_lock.joined_members.get(&message.sender) {
                                        Some(member) => member.disambiguated_name(),
                                        None => message.sender.to_string(),
                                    }
                                }
                                None => message.sender.to_string(),
                            }
                        };
                        let content: Element<_> = match &message.content {
                            MessageEventContent::Audio(audio) => {
                                Text::new(format!("Audio message: {}", audio.body))
                                    .color([0.2, 0.2, 0.2])
                                    .width(Length::Fill)
                                    .into()
                            }
                            MessageEventContent::Emote(emote) => {
                                Text::new(format!("{} {}", sender, emote.body))
                                    .width(Length::Fill)
                                    .into()
                            }
                            MessageEventContent::File(file) => {
                                Text::new(format!("File '{}'", file.body))
                                    .color([0.2, 0.2, 0.2])
                                    .width(Length::Fill)
                                    .into()
                            }
                            MessageEventContent::Image(image) => {
                                Text::new(format!("Image with description: {}", image.body))
                                    .width(Length::Fill)
                                    .into()
                            }
                            MessageEventContent::Notice(notice) => {
                                Text::new(&notice.body).width(Length::Fill).into()
                            }
                            MessageEventContent::ServerNotice(notice) => {
                                Text::new(&notice.body).width(Length::Fill).into()
                            }
                            MessageEventContent::Text(text) => {
                                Text::new(&text.body).width(Length::Fill).into()
                            }
                            MessageEventContent::Video(video) => {
                                Text::new(format!("Video: {}", video.body))
                                    .color([0.2, 0.2, 0.2])
                                    .into()
                            }
                            _ => Text::new("Unknown message type").into(),
                        };
                        let row = Row::new()
                            .spacing(5)
                            .push(Text::new(sender).color([0.0, 0.0, 1.0]))
                            .push(content)
                            .push(Text::new(format_systime(message.origin_server_ts)));
                        scroll = scroll.push(row);
                    }
                    _ => (),
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
                _ if sas.is_done() => Row::new()
                    .push(Text::new("Verification complete").width(Length::Fill))
                    .push(
                        Button::new(&mut self.sas_accept_button, Text::new("Close"))
                            .on_press(Message::VerificationClose),
                    ),
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
    /// Reset state for room
    ResetRoom(RoomId, RoomEntry),
    /// Get backfill for given room
    BackFill(RoomId),
    /// View messages from this room
    SelectRoom(RoomId),
    /// Set error message
    ErrorMessage(String),
    /// Close error message
    ClearError,
    /// Set how the room list is sorted
    SetSort(RoomSorting),
    /// Set verification flow
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
    /// Close verification bar
    VerificationClose,
    /// Matrix event received
    Sync(matrix::Event),
    /// Update the sync token to use
    SyncToken(String),
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
                    *self = Retrix::LoggedIn(MainView::new(client.clone(), session));
                    let joined = client.joined_rooms();
                    let read = block_on(async { joined.read().await });
                    let mut commands: Vec<Command<Message>> = Vec::new();
                    for (id, room) in read.iter() {
                        let id = id.clone();
                        let room = room.clone();
                        let client = client.clone();
                        let command = async move {
                            let room = room.read().await;
                            let mut entry = RoomEntry::default();

                            entry.direct = room.direct_target.clone();
                            // Display name calculation for DMs is bronk so we're doing it
                            // ourselves
                            match entry.direct {
                                Some(ref direct) => {
                                    let request = matrix_sdk::api::r0::profile::get_display_name::Request::new(direct);
                                    if let Ok(response) = client.send(request).await {
                                        if let Some(name) = response.displayname {
                                            entry.name = name;
                                        }
                                    }
                                }
                                None => entry.name = room.display_name(),
                            }
                            let messages = room
                                .messages
                                .iter()
                                .cloned()
                                .map(|event| match event {
                                    AnyPossiblyRedactedSyncMessageEvent::Redacted(e) => {
                                        AnyRoomEvent::RedactedMessage(
                                            e.into_full_event(id.clone()),
                                        )
                                    }
                                    AnyPossiblyRedactedSyncMessageEvent::Regular(e) => {
                                        AnyRoomEvent::Message(e.into_full_event(id.clone()))
                                    }
                                })
                            .collect();
                            entry.messages.messages = messages;
                            Message::ResetRoom(id, entry)
                        }.into();
                        commands.push(command)
                    }
                    return Command::batch(commands);
                }
                _ => (),
            },
            Retrix::LoggedIn(view) => match message {
                Message::ErrorMessage(e) => view.error = Some((e, Default::default())),
                Message::ClearError => view.error = None,
                Message::SetSort(s) => view.sorting = s,
                Message::ResetRoom(id, room) => {
                    view.rooms.insert(id, room).and(Some(())).unwrap_or(())
                }
                Message::SelectRoom(r) => view.selected = Some(r),
                Message::Sync(event) => match event {
                    matrix::Event::Room(event) => match event {
                        AnyRoomEvent::Message(event) => {
                            let room = view.rooms.entry(event.room_id().clone()).or_default();
                            room.messages.push(AnyRoomEvent::Message(event));
                        }
                        AnyRoomEvent::State(event) => match event {
                            AnyStateEvent::RoomCanonicalAlias(ref alias) => {
                                let room = view.rooms.entry(alias.room_id.clone()).or_default();
                                room.alias = alias.content.alias.clone();
                                room.messages.push(AnyRoomEvent::State(event));
                            }
                            AnyStateEvent::RoomName(ref name) => {
                                let room = view.rooms.entry(name.room_id.clone()).or_default();
                                room.display_name = name.content.name().map(String::from);
                                room.messages.push(AnyRoomEvent::State(event));
                            }
                            AnyStateEvent::RoomTopic(ref topic) => {
                                let room = view.rooms.entry(topic.room_id.clone()).or_default();
                                room.topic = topic.content.topic.clone();
                                room.messages.push(AnyRoomEvent::State(event));
                            }
                            ref any => {
                                // Ensure room exists
                                let room = view.rooms.entry(any.room_id().clone()).or_default();
                                room.messages.push(AnyRoomEvent::State(event));
                            }
                        },
                        _ => (),
                    },
                    matrix::Event::ToDevice(event) => match event {
                        AnyToDeviceEvent::KeyVerificationStart(start) => {
                            let client = view.client.clone();
                            return Command::perform(
                                async move {
                                    tokio::time::delay_for(std::time::Duration::from_secs(2)).await;
                                    client.get_verification(&start.content.transaction_id).await
                                },
                                Message::SetVerification,
                            );
                        }
                        AnyToDeviceEvent::KeyVerificationCancel(cancel) => {
                            return async { Message::VerificationCancelled(cancel.content.code) }
                                .into();
                        }
                        _ => (),
                    },
                    matrix::Event::Token(token) => {
                        view.sync_token = token;
                    }
                },
                Message::SetVerification(v) => view.sas = v,
                Message::VerificationAccept => {
                    let sas = match &view.sas {
                        Some(sas) => sas.clone(),
                        None => return Command::none(),
                    };
                    return Command::perform(
                        async move { sas.accept().await },
                        |result| match result {
                            Ok(()) => Message::VerificationAccepted,
                            Err(e) => Message::ErrorMessage(e.to_string()),
                        },
                    );
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
                    return Command::perform(
                        async move { sas.cancel().await },
                        |result| match result {
                            Ok(()) => Message::VerificationCancelled(VerificationCancelCode::User),
                            Err(e) => Message::ErrorMessage(e.to_string()),
                        },
                    );
                }
                Message::VerificationCancelled(code) => {
                    view.sas = None;
                    return async move { Message::ErrorMessage(code.as_str().to_owned()) }.into();
                }
                Message::VerificationClose => view.sas = None,
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
            },
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
