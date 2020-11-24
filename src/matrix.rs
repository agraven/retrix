use matrix_sdk::{reqwest::Url, Client, Session, SyncSettings};

pub type Error = Box<dyn std::error::Error>;

pub async fn login(
    username: &str,
    password: &str,
    server: &str,
) -> Result<(Client, Session), Error> {
    let url = Url::parse(server)?;
    let client = Client::new(url)?;

    let response = client
        .login(username, password, None, Some("retrix"))
        .await?;
    let session = Session {
        access_token: response.access_token,
        user_id: response.user_id,
        device_id: response.device_id,
    };
    client.sync(SyncSettings::new()).await;

    Ok((client, session))
}
