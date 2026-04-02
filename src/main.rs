mod calendar_bot;
mod bot_error;

use crate::calendar_bot::CalendarBot;
use dotenv::dotenv;

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
