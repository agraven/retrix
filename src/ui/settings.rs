//! Settings view.

use iced::{Button, Column, Container, Element, Length, Radio, Row, Text, TextInput};

use super::{Message, RoomSorting};

/// Settings menu
#[derive(Clone, Default, Debug)]
pub struct SettingsView {
    /// Display name to set
    pub display_name: String,
    /// Are we saving the display name?
    pub saving_name: bool,

    /// Display name text input
    pub display_name_input: iced::text_input::State,
    /// Button to set display name
    pub display_name_button: iced::button::State,

    /// Path to import encryption keys from
    pub key_path: String,
    /// Password to decrypt the keys with
    pub key_password: String,

    /// Encryption key path entry
    pub key_path_input: iced::text_input::State,
    /// Entry for key password
    pub key_password_input: iced::text_input::State,
    /// Button to import keys
    pub key_import_button: iced::button::State,
    /// Button  to close settings view
    pub close_button: iced::button::State,
}

impl SettingsView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn view(&mut self, sort: RoomSorting) -> Element<Message> {
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
