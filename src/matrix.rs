use std::{
    convert::TryFrom,
    time::{Duration, SystemTime},
};

use async_stream::stream;
use matrix_sdk::{
    api::r0::{account::register::Request as RegistrationRequest, uiaa::AuthData},
    events::{
        room::message::{MessageEvent, MessageEventContent},
        AnyMessageEvent, AnyRoomEvent, AnySyncRoomEvent, AnyToDeviceEvent,
    },
    identifiers::{DeviceId, EventId, ServerName, UserId},
    reqwest::Url,
    Client, ClientConfig, LoopCtrl, SyncSettings,
};
use serde::{Deserialize, Serialize};

pub type Error = anyhow::Error;

// Needed to be able to serialize `Session`s. Should be done with serde remote.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Session {
    access_token: String,
    pub user_id: UserId,
    pub device_id: Box<DeviceId>,
    pub homeserver: String,
}

impl From<Session> for matrix_sdk::Session {
    fn from(s: Session) -> Self {
        Self {
            access_token: s.access_token,
            user_id: s.user_id,
            device_id: s.device_id,
        }
    }
}

pub async fn signup(
    username: &str,
    password: &str,
    server: &str,
    device_name: Option<&str>,
) -> Result<(Client, Session), Error> {
    let url = Url::parse(server)?;
    let client = client(url)?;

    let mut request = RegistrationRequest::new();
    request.username = Some(username);
    request.password = Some(password);
    request.initial_device_display_name = Some(device_name.unwrap_or("retrix"));
    request.inhibit_login = false;

    // Get UIAA session key
    let uiaa = match client.register(request.clone()).await {
        Err(e) => match e.uiaa_response().cloned() {
            Some(uiaa) => uiaa,
            None => return Err(anyhow::anyhow!("Missing UIAA response")),
        },
        Ok(_) => {
            return Err(anyhow::anyhow!("Missing UIAA response"));
        }
    };
    // Get the first step in the authentication flow (we're ignoring the rest)
    let stages = uiaa.flows.get(0);
    let kind = stages.and_then(|flow| flow.stages.get(0)).cloned();

    // Set authentication data, fallback to password type
    request.auth = Some(AuthData::DirectRequest {
        kind: kind.as_deref().unwrap_or("m.login.password"),
        session: uiaa.session.as_deref(),
        auth_parameters: Default::default(),
    });

    let response = client.register(request).await?;
    client.sync_once(SyncSettings::new()).await?;

    let session = Session {
        access_token: response.access_token.unwrap(),
        user_id: response.user_id,
        device_id: response.device_id.unwrap(),
        homeserver: server.to_owned(),
    };

    Ok((client, session))
}

/// Login with credentials, creating a new authentication session
pub async fn login(
    username: &str,
    password: &str,
    server: &str,
    device_name: Option<&str>,
) -> Result<(Client, Session), Error> {
    let url = Url::parse(server)?;
    let client = client(url)?;

    let response = client
        .login(
            username,
            password,
            None,
            Some(device_name.unwrap_or("retrix")),
        )
        .await?;
    let session = Session {
        access_token: response.access_token,
        user_id: response.user_id,
        device_id: response.device_id,
        homeserver: server.to_owned(),
    };
    write_session(&session)?;
    client.sync_once(SyncSettings::new()).await?;

    Ok((client, session))
}

pub async fn restore_login(session: Session) -> Result<(Client, Session), Error> {
    let url = Url::parse(&session.homeserver)?;
    let client = client(url)?;

    client.restore_login(session.clone().into()).await?;
    client.sync_once(SyncSettings::new()).await?;

    Ok((client, session))
}

/// Create a matrix client handler with the desired configuration
fn client(url: Url) -> Result<Client, matrix_sdk::Error> {
    let config = ClientConfig::new().store_path(&dirs::config_dir().unwrap().join("retrix"));
    Client::new_with_config(url, config)
}

/// File path to store session data in
fn session_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap()
        .join("retrix")
        .join("session.toml")
}

/// Read session data from config file
pub fn get_session() -> Result<Option<Session>, Error> {
    let path = session_path();
    if !path.is_file() {
        return Ok(None);
    }
    let session: Session = toml::from_slice(&std::fs::read(path)?)?;
    Ok(Some(session))
}

/// Save session data to config file
fn write_session(session: &Session) -> Result<(), Error> {
    let serialized = toml::to_string(&session)?;
    std::fs::write(session_path(), serialized)?;

    Ok(())
}

/// Break down an mxc url to its authority and path
pub fn parse_mxc(url: &str) -> Result<(Box<ServerName>, String), Error> {
    let url = Url::parse(&url)?;
    anyhow::ensure!(url.scheme() == "mxc", "Not an mxc url");
    let host = url.host_str().ok_or_else(|| anyhow::anyhow!("url"))?;
    let server_name: Box<ServerName> = <&ServerName>::try_from(host)?.into();
    let path = url.path_segments().and_then(|mut p| p.next());
    match path {
        Some(path) => Ok((server_name, path.to_owned())),
        _ => Err(anyhow::anyhow!("Invalid mxc url")),
    }
}

pub struct MatrixSync {
    client: matrix_sdk::Client,
    join: Option<tokio::task::JoinHandle<()>>,
    //id: String,
}

impl MatrixSync {
    pub fn subscription(client: matrix_sdk::Client) -> iced::Subscription<Event> {
        iced::Subscription::from_recipe(MatrixSync { client, join: None })
    }
}

#[derive(Clone, Debug)]
pub enum Event {
    Room(AnyRoomEvent),
    ToDevice(AnyToDeviceEvent),
    Token(String),
}

/*pub enum Sync {
    MessageInvited()
    MessageJoined(RoomId, AnyRoomEvent),
    MessageLeft(RoomId, AnyRoomEvent),
}

struct Emitter {
    client: Client,
    sender: tokio::sync::mpsc::Sender<Sync>,
}

#[async_trait]
impl matrix_sdk::EventEmitter for Emitter {
    async fn on_room_message(
        &self,
        room: RoomState,
        message: &SyncMessageEvent<MessageEventContent>,
    ) {
        let id = room.room_id().to_owned();
        let full = AnyRoomEvent::Message(
            AnyMessageEvent::RoomMessage(
                message
                .to_owned()
                .into_full_event(id.clone()),
            ),
        );
        if let RoomState::Joined(room) = room {
            self.sender
                .send(Sync::MessageJoined(id, full))
                .await
                .ok();
        }
    }

    async fn on_room_member(&self, room: RoomState, member: &SyncStateEvent<MemberEventContent>) {

    }
}*/

impl<H, I> iced_futures::subscription::Recipe<H, I> for MatrixSync
where
    H: std::hash::Hasher,
{
    type Output = Event;

    fn hash(&self, state: &mut H) {
        use std::hash::Hash;

        std::any::TypeId::of::<Self>().hash(state);
        //self.id.hash(state);
    }

    fn stream(
        mut self: Box<Self>,
        _input: iced_futures::BoxStream<I>,
    ) -> iced_futures::BoxStream<Self::Output> {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let client = self.client.clone();
        let join = tokio::task::spawn(async move {
            client
                .sync_with_callback(
                    SyncSettings::new()
                        .token(client.sync_token().await.unwrap())
                        .timeout(Duration::from_secs(30)),
                    |response| async {
                        //sender.send(Event::Token(response.next_batch)).ok();
                        for (id, room) in response.rooms.join {
                            for event in room.state.events {
                                let id = id.clone();
                                sender
                                    .send(Event::Room(AnyRoomEvent::State(
                                        event.into_full_event(id),
                                    )))
                                    .ok();
                            }
                            for event in room.timeline.events {
                                let id = id.clone();
                                let event = match event {
                                    AnySyncRoomEvent::Message(e) => {
                                        AnyRoomEvent::Message(e.into_full_event(id))
                                    }
                                    AnySyncRoomEvent::State(e) => {
                                        AnyRoomEvent::State(e.into_full_event(id))
                                    }
                                    AnySyncRoomEvent::RedactedMessage(e) => {
                                        AnyRoomEvent::RedactedMessage(e.into_full_event(id))
                                    }
                                    AnySyncRoomEvent::RedactedState(e) => {
                                        AnyRoomEvent::RedactedState(e.into_full_event(id))
                                    }
                                };
                                sender.send(Event::Room(event)).ok();
                            }
                        }
                        for event in response.to_device.events {
                            sender.send(Event::ToDevice(event)).ok();
                        }
                        LoopCtrl::Continue
                    },
                )
                .await;
        });
        self.join = Some(join);
        let stream = stream! {
            while let Some(item) = receiver.recv().await {
                yield item;
            }
        };
        Box::pin(stream)
    }
}

pub trait AnyRoomEventExt {
    /// Gets the event id of the underlying event
    fn event_id(&self) -> &EventId;
    /// Gets the ´origin_server_ts` member of the underlying event
    fn origin_server_ts(&self) -> SystemTime;
    /// Gets the mxc url in a message event if there is noe
    fn image_url(&self) -> Option<String>;
}

impl AnyRoomEventExt for AnyRoomEvent {
    fn event_id(&self) -> &EventId {
        match self {
            AnyRoomEvent::Message(e) => e.event_id(),
            AnyRoomEvent::State(e) => e.event_id(),
            AnyRoomEvent::RedactedMessage(e) => e.event_id(),
            AnyRoomEvent::RedactedState(e) => e.event_id(),
        }
    }
    fn origin_server_ts(&self) -> SystemTime {
        match self {
            AnyRoomEvent::Message(e) => e.origin_server_ts(),
            AnyRoomEvent::State(e) => e.origin_server_ts(),
            AnyRoomEvent::RedactedMessage(e) => e.origin_server_ts(),
            AnyRoomEvent::RedactedState(e) => e.origin_server_ts(),
        }
        .to_owned()
    }
    fn image_url(&self) -> Option<String> {
        match self {
            AnyRoomEvent::Message(message) => message.image_url(),
            _ => None,
        }
    }
}

pub trait AnyMessageEventExt {
    fn image_url(&self) -> Option<String>;
}

impl AnyMessageEventExt for AnyMessageEvent {
    fn image_url(&self) -> Option<String> {
        match self {
            AnyMessageEvent::RoomMessage(MessageEvent {
                content: MessageEventContent::Image(ref image),
                ..
            }) => image.url.clone(),
            _ => None,
        }
    }
}
