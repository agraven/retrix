use matrix_sdk::{
    identifiers::DeviceId, identifiers::UserId, reqwest::Url, Client, ClientConfig, Session,
    SyncSettings,
};
use serde::{Deserialize, Serialize};

pub type Error = anyhow::Error;

// Needed to be able to serialize `Session`s. Should be done with serde remote.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionWrapper {
    access_token: String,
    user_id: UserId,
    device_id: Box<DeviceId>,
}

impl From<SessionWrapper> for Session {
    fn from(s: SessionWrapper) -> Self {
        Self {
            access_token: s.access_token,
            user_id: s.user_id,
            device_id: s.device_id,
        }
    }
}

impl From<Session> for SessionWrapper {
    fn from(s: Session) -> Self {
        Self {
            access_token: s.access_token,
            user_id: s.user_id,
            device_id: s.device_id,
        }
    }
}

pub async fn login(
    username: &str,
    password: &str,
    server: &str,
) -> Result<(Client, Session), Error> {
    let url = Url::parse(server)?;
    let config = ClientConfig::new().store_path(&dirs::config_dir().unwrap().join("retrix"));
    let client = Client::new_with_config(url, config)?;

    let session = match get_session()? {
        Some(session) => {
            client.restore_login(session.clone()).await?;
            session
        }
        None => {
            let response = client
                .login(username, password, None, Some("retrix"))
                .await?;
            let session = Session {
                access_token: response.access_token,
                user_id: response.user_id,
                device_id: response.device_id,
            };
            write_session(session.clone())?;
            session
        }
    };
    client.sync_once(SyncSettings::new()).await?;

    Ok((client, session))
}

/// File path to store session data in
fn session_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap()
        .join("retrix")
        .join("session.toml")
}

/// Read session data from config file
fn get_session() -> Result<Option<Session>, Error> {
    let path = session_path();
    if !path.is_file() {
        return Ok(None);
    }
    let session: SessionWrapper = toml::from_slice(&std::fs::read(path)?)?;
    Ok(Some(session.into()))
}

/// Save session data to config file
fn write_session(session: Session) -> Result<(), Error> {
    let session: SessionWrapper = session.into();
    let path = session_path();

    let serialized = toml::to_string(&session)?;
    std::fs::write(path, serialized)?;

    Ok(())
}
