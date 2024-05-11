#![windows_subsystem = "windows"]

use log::{debug, error}; // hide console on windows
mod constants;
mod crypto;
mod connection_manager;
mod conference_manager;
mod state_manager;
mod cli_ui;
mod gtk_ui;

#[async_std::main]
async fn main() {
    env_logger::init();
    let mut use_cli = false;
    let mut server_address = "localhost:7667".to_string();

    let mut args = std::env::args().skip(1); // skip binary name
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--cli" => use_cli = true,
            "--server-address" => {
                if let Some(server_address_arg) = args.next() {
                    server_address = server_address_arg;
                }
            }
            _ => {
                error!("Unknown argument: {}", arg);
                return;
            }
        }
    }

    debug!("Connecting to the server at {}", server_address);

    if use_cli {
        let mut ui = cli_ui::CLII_UI::new(server_address);
        ui.start_ui().await;
    } else {
        gtk_ui::main_window::start_gtk_ui(server_address);
    }
}
