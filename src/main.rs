extern crate dirs_next as dirs;

#[cfg(unix)]
use std::{fs::Permissions, os::unix::fs::PermissionsExt};

use iced::Application;

pub mod matrix;
pub mod ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let config_dir = dirs::config_dir().unwrap().join("retrix");
    // Make sure config dir exists and is not accessible by other users.
    if !config_dir.is_dir() {
        std::fs::create_dir(&config_dir)?;
        #[cfg(unix)]
        std::fs::set_permissions(&config_dir, Permissions::from_mode(0o700))?;
    }

    ui::Retrix::run(iced::Settings::default())?;

    Ok(())
}
