use matrix_sdk::{reqwest::Url, Client, SyncSettings};

async fn login(
    username: &str,
    password: &str,
    server: &str,
) -> Result<Client, Box<dyn std::error::Error>> {
    let url = Url::parse(server)?;
    let client = Client::new(url)?;

    client
        .login(username, password, None, Some("retrix"))
        .await?;
    client.sync(SyncSettings::new()).await;

    Ok(client)
}
