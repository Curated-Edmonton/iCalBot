mod bot;
mod entities;
mod errors;
mod icalendar;
mod state;
mod web;

use dotenv::dotenv;
use std::sync::Arc;
use std::time::Duration;
use tokio::{sync::watch, task::JoinSet};

#[tokio::main]
async fn main() {
    // Load environment variables from a .env file, if it exists
    dotenv().ok();

    let app_state = Arc::new(
        state::AppState::new()
            .await
            .expect("Failed to initialize app state"),
    );

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Run bot and web as independently managed tasks.
    let web_state = Arc::clone(&app_state);
    let bot_state = Arc::clone(&app_state);
    let mut tasks = JoinSet::new();

    tasks.spawn(async move {
        bot::CalendarBot::new(bot_state)
            .await
            .expect("Failed to initialize the bot")
            .run(shutdown_rx)
            .await
            .map_err(|e| format!("The bot encountered an error while running: {}", e))
    });

    let web_shutdown = shutdown_tx.subscribe();
    tasks.spawn(async move {
        web::listen_and_serve(web_state, web_shutdown)
            .await
            .map_err(|e| format!("The HTTP server encountered an error while running: {}", e))
    });

    // Trigger global shutdown when Ctrl+C arrives, or if either task exits.
    tokio::select! {
        ctrlc = tokio::signal::ctrl_c() => {
            match ctrlc {
                Ok(()) => println!("Ctrl+C received. Initiating graceful shutdown."),
                Err(e) => eprintln!("Failed to listen for Ctrl+C: {}", e),
            }
        }
        maybe_result = tasks.join_next() => {
            match maybe_result {
                Some(Ok(Ok(()))) => println!("A component exited cleanly. Initiating shutdown of remaining components."),
                Some(Ok(Err(e))) => eprintln!("A component exited with error: {}", e),
                Some(Err(e)) => eprintln!("A component task panicked or was cancelled: {}", e),
                None => println!("No running components remain."),
            }
        }
    }

    if shutdown_tx.send(true).is_err() {
        eprintln!("Failed to broadcast shutdown signal: no active receivers");
    }

    let shutdown_deadline = tokio::time::sleep(Duration::from_secs(10));
    tokio::pin!(shutdown_deadline);

    while !tasks.is_empty() {
        tokio::select! {
            _ = &mut shutdown_deadline => {
                eprintln!("Graceful shutdown timed out after 10 seconds. Aborting remaining tasks.");
                tasks.abort_all();
                break;
            }
            result = tasks.join_next() => {
                match result {
                    Some(Ok(Ok(()))) => {}
                    Some(Ok(Err(e))) => eprintln!("Component stopped with error during shutdown: {}", e),
                    Some(Err(e)) => eprintln!("Component task panicked or was cancelled during shutdown: {}", e),
                    None => break,
                }
            }
        }
    }

    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => eprintln!("Component stopped with error after abort: {}", e),
            Err(e) => eprintln!("Component task panicked or was cancelled after abort: {}", e),
        }
    }

    println!("Shutdown complete.");
}
