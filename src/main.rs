mod constants;
mod crypto;
mod connection_manager;
mod conference_manager;
mod state_manager;
mod cli_ui;

#[async_std::main]
async fn main() {
    env_logger::init();
    let mut ui = cli_ui::CLII_UI::new("localhost:7667".to_string());
    ui.start_ui().await;
}
