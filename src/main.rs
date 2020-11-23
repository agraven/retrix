use iced::Sandbox;

pub mod matrix;
pub mod ui;

fn main() {
    ui::Retrix::run(iced::Settings::default())
}
