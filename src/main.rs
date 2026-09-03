mod api;
mod config;
mod ui;
mod app;

use ratatui::{DefaultTerminal, Frame};

use crate::api::Client;
use crate::app::App;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let terminal = ratatui::init();
    let app_result = App::default().run(terminal).await;
    ratatui::restore();
    app_result
}
