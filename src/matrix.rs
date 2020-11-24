use matrix_sdk::{
    identifiers::DeviceId, identifiers::UserId, reqwest::Url, Client, ClientConfig, SyncSettings,
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
