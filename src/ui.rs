use std::{
    collections::{BTreeMap, HashSet},
    time::SystemTime,
};

use futures::executor::block_on;
use iced::{
    Align, Application, Button, Column, Command, Container, Element, Image, Length, Row, Rule,
    Scrollable, Subscription, Text, TextInput,
};
use matrix_sdk::{
    api::r0::{
        media::get_content::Request as ImageRequest,
        message::get_message_events::{Request as MessageRequest, Response as MessageResponse},
    },
    events::{
        key::verification::cancel::CancelCode as VerificationCancelCode,
        room::{
            member::MembershipState,
            message::{MessageEventContent, MessageType, Relation},
        },
        AnyMessageEvent, AnyMessageEventContent, AnyRoomEvent, AnyStateEvent, AnyToDeviceEvent,
    },
    identifiers::{EventId, RoomAliasId, RoomId, UserId},
};

use crate::matrix::{self, AnyMessageEventExt, AnyRoomEventExt};

pub mod prompt;
pub mod settings;
pub mod theme;

use prompt::{PromptAction, PromptView};
use settings::SettingsView;

const THUMBNAIL_SIZE: u32 = 48;

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
    pub name: String,
    /// Room topic
    pub topic: String,
    /// Canonical alias
    pub alias: Option<RoomAliasId>,
    /// Defined display name
    pub display_name: Option<String>,
    /// mxc url for the rooms avatar
    pub avatar: Option<String>,
    /// Person we're in a direct message with
    pub direct: Option<UserId>,
    /// Cache of messages
    pub messages: MessageBuffer,
}

impl RoomEntry {
    pub async fn from_sdk(room: &matrix_sdk::room::Joined) -> Self {
        Self {
            direct: room.direct_target(),
            name: room.display_name().await.unwrap(),
            topic: room.topic().unwrap_or_default(),
            alias: room.canonical_alias(),
            avatar: room.avatar_url(),
            ..Default::default()
        }
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
    /// Whether we're awaiting for backfill to be received
    loading: bool,
}

impl MessageBuffer {
    /// Sorts the messages by send time
    fn sort(&mut self) {
        self.messages
            .sort_unstable_by_key(|msg| msg.origin_server_ts())
    }

    /// Gets the send time of the most recently sent message
    fn update_time(&mut self) {
        self.updated = match self.messages.last() {
            Some(message) => message.origin_server_ts(),
            None => SystemTime::UNIX_EPOCH,
        };
    }

    fn remove(&mut self, id: &EventId) {
        self.messages.retain(|e| e.event_id() != id);
        self.known_ids.remove(&id);
    }

    /// Add a message to the buffer.
    pub fn push(&mut self, event: AnyRoomEvent) {
        self.known_ids.insert(event.event_id().clone());
        if let AnyRoomEvent::Message(AnyMessageEvent::RoomMessage(
            matrix_sdk::events::MessageEvent {
                content:
                    MessageEventContent {
                        relates_to: Some(Relation::Replacement(ref replacement)),
                        ..
                    },
                ..
            },
        )) = event
        {
            self.remove(&replacement.event_id);
        }
        if let AnyRoomEvent::Message(AnyMessageEvent::RoomRedaction(ref redaction)) = event {
            self.remove(&redaction.redacts);
        }
        self.messages.push(event);
        self.sort();
        self.update_time();
    }

    /// Adds several messages to the buffer
    pub fn append(&mut self, mut events: Vec<AnyRoomEvent>) {
        events.retain(|e| !self.known_ids.contains(e.event_id()));
        for event in events.iter() {
            // Handle replacement
            if let AnyRoomEvent::Message(AnyMessageEvent::RoomMessage(
                matrix_sdk::events::MessageEvent {
                    content:
                        MessageEventContent {
                            relates_to: Some(Relation::Replacement(replacement)),
                            ..
                        },
                    ..
                },
            )) = event
            {
                self.remove(&replacement.event_id);
            }
            if let AnyRoomEvent::Message(AnyMessageEvent::RoomRedaction(redaction)) = event {
                self.remove(&redaction.redacts);
            }
            self.known_ids.insert(event.event_id().clone());
        }
        self.messages.append(&mut events);
        self.sort();
        self.update_time();
    }

    /// Whather the message buffer has the room creation event
    pub fn has_beginning(&self) -> bool {
        self.messages
            .iter()
            .any(|e| matches!(e, AnyRoomEvent::State(AnyStateEvent::RoomCreate(_))))
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
            loading: false,
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
    /// A map of mxc urls to image data
    images: BTreeMap<String, iced::image::Handle>,
    /// A map of mxc urls to image thumbnails
    thumbnails: BTreeMap<String, iced::image::Handle>,

    /// Room list entries for direct conversations
    dm_buttons: Vec<iced::button::State>,
    /// Room list entries for group conversations
    group_buttons: Vec<iced::button::State>,
    /// Room list scrollbar state
    room_scroll: iced::scrollable::State,
    /// Message view scrollbar state
    message_scroll: iced::scrollable::State,
    /// Backfill fetch button state
    backfill_button: iced::button::State,
    /// Button to go the room a tombstone points to
    tombstone_button: iced::button::State,
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
            images: Default::default(),
            thumbnails: Default::default(),
            room_scroll: Default::default(),
            message_scroll: Default::default(),
            backfill_button: Default::default(),
            tombstone_button: Default::default(),
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

    pub fn room<'a>(&'a self, id: &RoomId) -> Result<&'a RoomEntry, Command<Message>> {
        match self.rooms.get(&id) {
            Some(room) => Ok(room),
            None => {
                let id = id.clone();
                let client = self.client.clone();
                let cmd = async move {
                    let room = client.get_joined_room(&id).unwrap();
                    let entry = RoomEntry::from_sdk(&room).await;
                    Message::ResetRoom(id, entry)
                }
                .into();
                Err(cmd)
            }
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

        let client = &self.client;
        let rooms = &self.rooms;
        // Group by DM and group conversation
        #[allow(clippy::type_complexity)]
        let (mut dm_rooms, mut group_rooms): (
            Vec<(&RoomId, &RoomEntry)>,
            Vec<(&RoomId, &RoomEntry)>,
        ) = rooms
            .iter()
            // Hide if have joined the room the tombstone points to
            .filter(|(id, _)| {
                !client
                    .get_joined_room(id)
                    .and_then(|j| j.tombstone())
                    .map(|t| rooms.contains_key(&t.replacement_room))
                    .unwrap_or(false)
            })
            .partition(|(_, room)| room.direct.is_some());
        // Sort
        for list in [&mut dm_rooms, &mut group_rooms].iter_mut() {
            match self.sorting {
                RoomSorting::Alphabetic => {
                    list.sort_unstable_by_key(|(_, room)| room.name.to_uppercase())
                }
                RoomSorting::Recent => list.sort_unstable_by(|(_, a), (_, b)| {
                    a.messages.updated.cmp(&b.messages.updated).reverse()
                }),
            };
        }
        // Make sure button handler list has appropriate length
        self.dm_buttons
            .resize_with(dm_rooms.len(), Default::default);
        self.group_buttons
            .resize_with(group_rooms.len(), Default::default);
        // Create buttons
        let thumbnails = &self.thumbnails;
        let images = &self.images;
        let dm_buttons: Vec<Button<_>> = self
            .dm_buttons
            .iter_mut()
            .enumerate()
            .map(|(idx, button)| {
                // TODO: highlight selected
                let (id, room) = unsafe { dm_rooms.get_unchecked(idx) };
                let name = if room.name.is_empty() {
                    "Empty room"
                } else {
                    &room.name
                };
                let mut row = Row::new().align_items(Align::Center);
                if let Some(ref url) = room.avatar {
                    if let Some(handle) = thumbnails.get(url) {
                        row = row.push(
                            Image::new(handle.clone())
                                .width(20.into())
                                .height(20.into()),
                        );
                    }
                }
                Button::new(button, row.push(Text::new(name)))
                    .width(300.into())
                    .on_press(Message::SelectRoom(id.to_owned().to_owned()))
            })
            .collect();
        let room_buttons: Vec<Button<_>> = self
            .group_buttons
            .iter_mut()
            .enumerate()
            .map(|(idx, button)| {
                let (id, room) = unsafe { group_rooms.get_unchecked(idx) };
                let name = if room.name.is_empty() {
                    "Missing name"
                } else {
                    &room.name
                };
                let mut row = Row::new().align_items(Align::Center);
                if let Some(ref url) = room.avatar {
                    if let Some(handle) = images.get(url) {
                        row = row.push(
                            Image::new(handle.clone())
                                .width(20.into())
                                .height(20.into()),
                        );
                    }
                }
                Button::new(button, row.push(Text::new(name)))
                    .width(300.into())
                    .on_press(Message::SelectRoom(id.to_owned().to_owned()))
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
            Some(ref selected) => match (
                self.rooms.get(selected),
                self.client.get_joined_room(selected),
            ) {
                (Some(room), Some(joined)) => Some((room, joined)),
                _ => None,
            },
            None => None,
        };
        if let Some((room, joined)) = selected_room {
            // Include user id or canonical alias in title when appropriate
            let title = if let Some(ref direct) = room.direct {
                format!("{} ({})", &room.name, direct)
            } else if let Some(ref alias) = room.alias {
                format!("{} ({})", &room.name, alias)
            } else {
                room.name.clone()
            };

            let mut title_row = Row::new().align_items(Align::Center);
            if let Some(handle) = room.avatar.as_deref().and_then(|a| images.get(a)) {
                title_row = title_row.push(
                    Image::new(handle.to_owned())
                        .width(24.into())
                        .height(24.into()),
                );
            }
            message_col = message_col
                .push(title_row.push(Text::new(title).size(25)))
                .push(Rule::horizontal(2));
            let mut scroll = Scrollable::new(&mut self.message_scroll)
                .scrollbar_width(2)
                .spacing(4)
                .height(Length::Fill);
            // Backfill button or loading message
            let backfill: Element<_> = if room.messages.loading {
                Text::new("Loading...").into()
            } else if room.messages.has_beginning() {
                let creation = joined.create_content().unwrap();
                let mut col =
                    Column::new().push(Text::new("This is the beginning of room history"));
                if let Some(prevous) = creation.predecessor {
                    col = col.push(
                        Button::new(&mut self.backfill_button, Text::new("Go to older version"))
                            .on_press(Message::SelectRoom(prevous.room_id)),
                    );
                }
                col.into()
            } else {
                Button::new(&mut self.backfill_button, Text::new("Load more messages"))
                    .on_press(Message::BackFill(self.selected.clone().unwrap()))
                    .into()
            };
            scroll = scroll.push(Container::new(backfill).width(Length::Fill).center_x());
            // mxid of most recent sender
            let mut last_sender: Option<UserId> = None;
            // Rendered display name of most recent sender
            let mut sender = String::from("Unknown sender");
            // Messages
            for event in room.messages.messages.iter() {
                #[allow(clippy::single_match)]
                match event {
                    AnyRoomEvent::Message(AnyMessageEvent::RoomMessage(message)) => {
                        // Display sender if message is from new sender
                        if last_sender.as_ref() != Some(&message.sender) {
                            last_sender = Some(message.sender.clone());
                            sender = match block_on(async {
                                joined.get_member(&message.sender).await
                            }) {
                                Ok(Some(member)) => member.name().to_owned(),
                                _ => message.sender.to_string(),
                            };
                            scroll = scroll
                                .push(iced::Space::with_height(4.into()))
                                .push(Text::new(&sender).color([0.0, 0.0, 1.0]));
                        }
                        let content: Element<_> = match &message.content.msgtype {
                            MessageType::Audio(audio) => {
                                Text::new(format!("Audio message: {}", audio.body))
                                    .color([0.2, 0.2, 0.2])
                                    .width(Length::Fill)
                                    .into()
                            }
                            MessageType::Emote(emote) => {
                                Text::new(format!("* {} {}", sender, emote.body))
                                    .width(Length::Fill)
                                    .into()
                            }
                            MessageType::File(file) => Text::new(format!("File '{}'", file.body))
                                .color([0.2, 0.2, 0.2])
                                .width(Length::Fill)
                                .into(),
                            MessageType::Image(image) => {
                                if let Some(ref url) = image.url {
                                    match self.images.get(url) {
                                        Some(handle) => Container::new(
                                            Image::new(handle.to_owned())
                                                .width(800.into())
                                                .height(1200.into()),
                                        )
                                        .width(Length::Fill)
                                        .into(),
                                        None => {
                                            Text::new("Image not loaded").width(Length::Fill).into()
                                        }
                                    }
                                } else {
                                    Text::new("Encrypted images not supported yet")
                                        .width(Length::Fill)
                                        .into()
                                }
                            }
                            MessageType::Notice(notice) => {
                                Text::new(&notice.body).width(Length::Fill).into()
                            }
                            MessageType::ServerNotice(notice) => {
                                Text::new(&notice.body).width(Length::Fill).into()
                            }
                            MessageType::Text(text) => {
                                Text::new(&text.body).width(Length::Fill).into()
                            }
                            MessageType::Video(video) => {
                                Text::new(format!("Video: {}", video.body))
                                    .color([0.2, 0.2, 0.2])
                                    .into()
                            }
                            _ => Text::new("Unknown message type").into(),
                        };
                        let row = Row::new()
                            .spacing(5)
                            .push(content)
                            .push(Text::new(format_systime(message.origin_server_ts)));
                        scroll = scroll.push(row);
                    }
                    AnyRoomEvent::Message(AnyMessageEvent::RoomEncrypted(_encrypted)) => {
                        scroll = scroll.push(Text::new("Encrypted event").color([0.3, 0.3, 0.3]));
                    }
                    AnyRoomEvent::RedactedMessage(_) => {
                        scroll = scroll.push(Text::new("Deleted message").color([0.3, 0.3, 0.3]));
                    }
                    _ => (),
                }
            }
            // Tombstone
            if let Some(tombstone) = joined.tombstone() {
                let text = Text::new(format!(
                    "This room has been upgraded to a new version: {}",
                    tombstone.body
                ));
                let button =
                    Button::new(&mut self.tombstone_button, Text::new("Go to upgraded room"))
                        .on_press(Message::SelectRoom(tombstone.replacement_room));
                scroll = scroll.push(
                    Container::new(
                        Column::new()
                            .push(text)
                            .push(button)
                            .align_items(Align::Center),
                    )
                    .center_x()
                    .width(Length::Fill),
                );
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
                        row = row
                            .push(
                                Column::new()
                                    .align_items(iced::Align::Center)
                                    .push(Text::new(*emoji).size(32))
                                    .push(Text::new(*name)),
                            )
                            .spacing(5);
                    }
                    row.push(iced::Space::with_width(Length::Fill))
                        .push(
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

    fn update(&mut self, message: Message) -> Command<Message> {
        let view = self;
        match message {
            Message::ErrorMessage(e) => view.error = Some((e, Default::default())),
            Message::ClearError => view.error = None,
            Message::SetSort(s) => view.sorting = s,
            Message::ResetRoom(id, room) => {
                view.rooms.insert(id.clone(), room);
                return async move { Message::BackFill(id) }.into();
            }
            Message::SelectRoom(r) => {
                view.selected = Some(r.clone());
                if view.rooms.get(&r).unwrap().messages.messages.is_empty() {
                    return async move { Message::BackFill(r) }.into();
                }
            }
            Message::Sync(event) => match event {
                matrix::Event::Joined(event, joined) => match event {
                    AnyRoomEvent::Message(event) => {
                        let room = view.rooms.entry(event.room_id().clone()).or_default();
                        room.messages.push(AnyRoomEvent::Message(event.clone()));
                        let mut commands = Vec::new();
                        // Add fetch image command if the message has an image
                        let img_cmd = match event.image_url() {
                            Some(url) => async { Message::FetchImage(url) }.into(),
                            None => Command::none(),
                        };
                        commands.push(img_cmd);
                        // Set read marker if message is in selected room
                        /*if view.selected.as_ref() == Some(event.room_id()) {
                          let client = view.client.clone();
                          let marker_cmd = async move {
                          let result = client
                          .read_marker(
                          event.room_id(),
                          event.event_id(),
                          Some(event.event_id()),
                          )
                          .await
                          .err();
                          match result {
                          Some(err) => Message::ErrorMessage(err.to_string()),
                        // TODO: Make this an actual no-op
                        None => Message::Login,
                        }
                        }
                        .into();
                        commands.push(marker_cmd);
                        }*/

                        return Command::batch(commands);
                    }
                    AnyRoomEvent::State(event) => match event {
                        AnyStateEvent::RoomCanonicalAlias(ref alias) => {
                            let room = view.rooms.entry(alias.room_id.clone()).or_default();
                            room.alias = alias.content.alias.clone();
                            room.messages.push(AnyRoomEvent::State(event));
                        }
                        AnyStateEvent::RoomName(ref name) => {
                            let id = name.room_id.clone();
                            let room = view.rooms.entry(id.clone()).or_default();
                            room.display_name = name.content.name().map(String::from);
                            room.messages.push(AnyRoomEvent::State(event));
                            let client = view.client.clone();
                            return async move {
                                let joined = client.get_joined_room(&id).unwrap();
                                match joined.display_name().await {
                                    Ok(name) => Message::RoomName(id, name),
                                    Err(e) => Message::ErrorMessage(e.to_string()),
                                }
                            }
                            .into();
                        }
                        AnyStateEvent::RoomTopic(ref topic) => {
                            let room = view.rooms.entry(topic.room_id.clone()).or_default();
                            room.topic = topic.content.topic.clone();
                            room.messages.push(AnyRoomEvent::State(event));
                        }
                        AnyStateEvent::RoomAvatar(ref avatar) => {
                            let room = view.rooms.entry(avatar.room_id.clone()).or_default();
                            room.messages.push(AnyRoomEvent::State(event));
                            if let Some(url) = room.avatar.clone() {
                                room.avatar = Some(url.clone());
                                //return async { Message::FetchThumb(url) }.into();
                                return Command::none();
                            }
                        }
                        AnyStateEvent::RoomCreate(ref create) => {
                            // Add room to the entry list
                            let joined = view.client.get_joined_room(&create.room_id).unwrap();
                            let id = create.room_id.clone();
                            return async move {
                                let mut entry = RoomEntry::from_sdk(&joined).await;
                                entry.messages.push(AnyRoomEvent::State(event));
                                Message::ResetRoom(id, entry)
                            }
                            .into();
                        }
                        AnyStateEvent::RoomMember(ref member) => {
                            let room = view.rooms.entry(member.room_id.clone()).or_default();
                            let client = view.client.clone();
                            // If we left a room, remove it from the RoomEntry list
                            if member.state_key == view.session.user_id {
                                match member.content.membership {
                                    MembershipState::Join => {
                                        let id = member.room_id.clone();
                                        return async move {
                                            let joined = client.get_joined_room(&id).unwrap();
                                            let entry = RoomEntry::from_sdk(&joined).await;

                                            Message::ResetRoom(id, entry)
                                        }
                                        .into();
                                    }
                                    MembershipState::Leave => {
                                        // Deselect room if we're leaving selected room
                                        if view.selected.as_ref() == Some(&member.room_id) {
                                            view.selected = None;
                                        }
                                        view.rooms.remove(&member.room_id);
                                        return Command::none();
                                    }
                                    _ => (),
                                }
                            }
                            room.messages.push(AnyRoomEvent::State(event));
                        }
                        ref any => {
                            // Ensure room exists
                            let room = view.rooms.entry(any.room_id().clone()).or_default();
                            room.messages.push(AnyRoomEvent::State(event));
                        }
                    },
                    AnyRoomEvent::RedactedMessage(redacted) => {
                        let room = view.rooms.entry(redacted.room_id().clone()).or_default();
                        room.messages.push(AnyRoomEvent::RedactedMessage(redacted));
                    }
                    AnyRoomEvent::RedactedState(redacted) => {
                        let room = view.rooms.entry(redacted.room_id().clone()).or_default();
                        room.messages.push(AnyRoomEvent::RedactedState(redacted));
                    }
                },
                matrix::Event::ToDevice(event) => match event {
                    AnyToDeviceEvent::KeyVerificationStart(start) => {
                        let client = view.client.clone();
                        return Command::perform(
                            async move { client.get_verification(&start.content.transaction_id).await },
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
                _ => (),
            },
            Message::BackFill(id) => {
                let entry = view.rooms.entry(id.clone()).or_default();
                entry.messages.loading = true;
                let client = view.client.clone();
                let room = client.get_joined_room(&id).unwrap();
                let token = match entry.messages.end.clone() {
                    Some(end) => end,
                    None => room
                        .last_prev_batch()
                        .unwrap_or_else(|| view.sync_token.clone()),
                };
                return async move {
                    let mut request = MessageRequest::backward(&id, &token);
                    request.limit = matrix_sdk::uint!(30);
                    match room.messages(request).await {
                        Ok(response) => Message::BackFilled(id, response),
                        Err(e) => Message::ErrorMessage(e.to_string()),
                    }
                }
                .into();
            }
            Message::BackFilled(id, response) => {
                let room = view.rooms.get_mut(&id).unwrap();
                room.messages.loading = false;
                let events: Vec<AnyRoomEvent> = response
                    .chunk
                    .into_iter()
                    .filter_map(|e| e.deserialize().ok())
                    .chain(
                        response
                            .state
                            .into_iter()
                            .filter_map(|e| e.deserialize().ok().map(AnyRoomEvent::State)),
                    )
                    .collect();

                if let Some(start) = response.start {
                    room.messages.start = Some(start);
                }
                if let Some(end) = response.end {
                    room.messages.end = Some(end);
                }
                let commands: Vec<Command<_>> = events
                    .iter()
                    .filter_map(|e| e.image_url())
                    .map(|url| async { Message::FetchImage(url) }.into())
                    .collect();
                room.messages.append(events);
                return Command::batch(commands);
            }

            Message::FetchImage(url) => {
                let (server, path) = match matrix::parse_mxc(&url) {
                    Ok((server, path)) => (server, path),
                    Err(e) => return async move { Message::ErrorMessage(e.to_string()) }.into(),
                };
                let client = view.client.clone();
                return async move {
                    let request = ImageRequest::new(&path, &*server);
                    let response = client.send(request, None).await;
                    match response {
                        Ok(response) => Message::FetchedImage(
                            url,
                            iced::image::Handle::from_memory(response.file),
                        ),
                        Err(e) => Message::ErrorMessage(e.to_string()),
                    }
                }
                .into();
            }
            Message::FetchedImage(url, handle) => {
                view.images.insert(url, handle);
            }
            Message::FetchedThumbnail(url, handle) => {
                view.thumbnails.insert(url, handle);
            }
            Message::RoomName(id, name) => {
                if let Some(room) = view.rooms.get_mut(&id) {
                    room.name = name;
                }
            }
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
                return Command::perform(
                    async move { sas.confirm().await },
                    |result| match result {
                        Ok(()) => Message::VerificationConfirmed,
                        Err(e) => Message::ErrorMessage(e.to_string()),
                    },
                );
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
        };
        Command::none()
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum Retrix {
    Prompt(PromptView),
    AwaitLogin,
    LoggedIn(MainView),
}

#[derive(Debug, Clone)]
pub enum Message {
    /// Do nothing
    Noop,
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
    RoomName(RoomId, String),
    /// Get backfill for given room
    BackFill(RoomId),
    /// Received backfill
    BackFilled(RoomId, MessageResponse),
    /// Fetched a thumbnail
    FetchedThumbnail(String, iced::image::Handle),
    /// Fetch an image pointed to by an mxc url
    FetchImage(String),
    /// Fetched an image
    FetchedImage(String, iced::image::Handle),
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

    fn update(
        &mut self,
        message: Self::Message,
        _clipboard: &mut iced::Clipboard,
    ) -> Command<Self::Message> {
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
                    let view = PromptView {
                        error: Some(e),
                        ..PromptView::default()
                    };
                    *self = Retrix::Prompt(view);
                }
                Message::LoggedIn(client, session) => {
                    *self = Retrix::LoggedIn(MainView::new(client.clone(), session));
                    let mut commands: Vec<Command<Message>> = Vec::new();
                    for room in client.joined_rooms().into_iter() {
                        let room = std::sync::Arc::new(room);
                        let r = room.clone();
                        let command: Command<_> = async move {
                            let entry = RoomEntry::from_sdk(&r).await;
                            Message::ResetRoom(r.room_id().to_owned(), entry)
                        }
                        .into();
                        commands.push(command);
                        // Fetch room avatar thumbnail if available
                        commands.push(
                            async move {
                                match room
                                    .avatar(Some(THUMBNAIL_SIZE), Some(THUMBNAIL_SIZE))
                                    .await
                                {
                                    Ok(Some(avatar)) => Message::FetchedThumbnail(
                                        room.avatar_url().unwrap(),
                                        iced::image::Handle::from_memory(avatar),
                                    ),
                                    Ok(None) => Message::Noop,
                                    Err(e) => Message::ErrorMessage(e.to_string()),
                                }
                            }
                            .into(),
                        )
                    }
                    return Command::batch(commands);
                }
                _ => (),
            },
            Retrix::LoggedIn(view) => return view.update(message),
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
    let offset = time::UtcOffset::try_current_local_offset().unwrap_or(time::UtcOffset::UTC);
    let time = time::OffsetDateTime::from(time).to_offset(offset);
    let today = time::OffsetDateTime::now_utc().to_offset(offset).date();
    // Display
    if time.date() == today {
        time.format("%T")
    } else {
        time.format("%F %T")
    }
}
