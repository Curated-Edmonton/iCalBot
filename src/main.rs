mod errors;
mod entities;
mod bot;
mod icalendar;

use dotenv::dotenv;
use crate::bot::CalendarBot;

#[tokio::main]
async fn main() {
    // Load environment variables from a .env file, if it exists
    dotenv().ok();

    // Create the bot instance
    let bot = CalendarBot::new().await
        .expect("Failed to initialize the bot");

    // Start the bot
    bot.run().await
        .expect("The bot encountered an error while running");

    println!("Bot has stopped running.");
}
