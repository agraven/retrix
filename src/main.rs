use iced::Application;

pub mod matrix;
pub mod ui;

#[tokio::main]
async fn main() {
    ui::Retrix::run(iced::Settings::default());
}
