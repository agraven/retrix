use matrix_sdk::{
    events::AnyRoomEvent, events::AnySyncRoomEvent, identifiers::DeviceId, identifiers::UserId,
    reqwest::Url, Client, ClientConfig, LoopCtrl, SyncSettings,
};
use serde::{Deserialize, Serialize};

pub type Error = anyhow::Error;

// Needed to be able to serialize `Session`s. Should be done with serde remote.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Session {
    access_token: String,
    user_id: UserId,
    device_id: Box<DeviceId>,
    homeserver: String,
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

/// Login with credentials, creating a new authentication session
pub async fn login(
    username: &str,
    password: &str,
    server: &str,
) -> Result<(Client, Session), Error> {
    let url = Url::parse(server)?;
    let client = client(url)?;

    let response = client
        .login(
            username,
            password,
            None,
            Some(&format!("retrix@{}", hostname::get()?.to_string_lossy())),
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

pub struct MatrixSync {
    client: matrix_sdk::Client,
    //id: String,
}

impl MatrixSync {
    pub fn subscription(client: matrix_sdk::Client) -> iced::Subscription<AnyRoomEvent> {
        iced::Subscription::from_recipe(MatrixSync { client })
    }
}

/*#[async_trait]
impl EventEmitter for Callback {
    async fn on_room_message(&self, room: SyncRoom, event: &SyncMessageEvent<MessageEventContent>) {
        let room_id = if let matrix_sdk::RoomState::Joined(arc) = room {
            let room = arc.read().await;
            room.room_id.clone()
        } else {
            return;
        };
        self.sender
            .send(event.clone().into_full_event(room_id))
            .ok();
    }
}*/

impl<H, I> iced_futures::subscription::Recipe<H, I> for MatrixSync
where
    H: std::hash::Hasher,
{
    type Output = AnyRoomEvent;

    fn hash(&self, state: &mut H) {
        use std::hash::Hash;

        std::any::TypeId::of::<Self>().hash(state);
        //self.id.hash(state);
    }

    fn stream(
        self: Box<Self>,
        _input: iced_futures::BoxStream<I>,
    ) -> iced_futures::BoxStream<Self::Output> {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let client = self.client.clone();
        tokio::task::spawn(async move {
            client
                .sync_with_callback(SyncSettings::new(), |response| async {
                    for (room_id, room) in response.rooms.join {
                        for event in room.timeline.events {
                            if let Ok(event) = event.deserialize() {
                                let room_id = room_id.clone();
                                let event = match event {
                                    AnySyncRoomEvent::Message(e) => {
                                        AnyRoomEvent::Message(e.into_full_event(room_id))
                                    }
                                    AnySyncRoomEvent::State(e) => {
                                        AnyRoomEvent::State(e.into_full_event(room_id))
                                    }
                                    AnySyncRoomEvent::RedactedMessage(e) => {
                                        AnyRoomEvent::RedactedMessage(e.into_full_event(room_id))
                                    }
                                    AnySyncRoomEvent::RedactedState(e) => {
                                        AnyRoomEvent::RedactedState(e.into_full_event(room_id))
                                    }
                                };
                                sender.send(event).ok();
                            }
                        }
                    }

                    LoopCtrl::Continue
                })
                .await;
        });
        Box::pin(receiver)
    }
}
