use async_std::task;
use constants::Result;
use protocol::enter_main_loop;

mod constants;
mod crypto;
mod protocol;
mod connection_manager;
mod conference_manager;

#[async_std::main]
async fn main() -> Result<()> {
    env_logger::init();
    task::block_on(enter_main_loop("localhost:7667"))
}
