mod bot_driver;
mod casino;
mod models;
mod routes;
mod storage;

use anyhow::Result;
use std::{env, time::Duration};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<()> {
    // Do NOT install a global tracing/log subscriber here.
    // Azalea uses Bevy internally, and Bevy wants to own the global logger.
    // Installing one here causes: "Could not set global logger... Consider disabling LogPlugin."
    if let Ok(delay_ms) = env::var("PEARL_STASIS_RESTART_DELAY_MS") {
        if let Ok(delay_ms) = delay_ms.parse::<u64>() {
            std::thread::sleep(Duration::from_millis(delay_ms));
        }
    }

    let config = storage::load_config();
    storage::save_config(&config)?;
    storage::ensure_users_file()?;
    let bot = bot_driver::BotController::new();
    let app = routes::app(bot);
    let bind = format!("{}:{}", config.gui_host, config.gui_port);
    let listener = TcpListener::bind(&bind).await?;
    println!("GUI running at http://{}", bind);
    axum::serve(listener, app).await?;
    Ok(())
}
