use std::path::PathBuf;

use serenity::async_trait;
use serenity::model::prelude::*;
use serenity::prelude::*;
use sqlx;

use crate::bot_error::BotError;


// Define a struct to hold the global state of the bot
pub struct CalendarBot {
    pub token: String,
    pub state_directory: PathBuf,
    pub database_url: String,
    pub database: sqlx::SqlitePool,
}


// And also act as a Type on which to implement Traits
impl CalendarBot {
    const BOT_TOKEN_ENV_VAR: &str = "BOT_TOKEN";

    // Either a file path to a SQLite database or a full connection string starting with "sqlite:"
    const DB_CONNECTION_STRING_ENV_VAR: &str = "DATABASE_URL";

    const STATE_DIRECTORY_ENV_VAR: &str = "BOT_STATE_DIRECTORY";
    const STATE_DIRECTORY_DEFAULT_VALUE: &str = ".discordcalendarbot";

    const INTENTS: [GatewayIntents; 1] = [GatewayIntents::GUILD_SCHEDULED_EVENTS];

    // Initialize the bot
    pub async fn new() -> Result<Self, BotError> {
        // Get the Bot Token
        let token = match std::env::var(Self::BOT_TOKEN_ENV_VAR) {
            Ok(token) => token,
            Err(_) => return Err(BotError::new(format!("Environment variable {} is not set", Self::BOT_TOKEN_ENV_VAR))),
        };

        // Get the State Directory
        let state_directory = match std::env::var(Self::STATE_DIRECTORY_ENV_VAR) {
            Ok(dir) => PathBuf::from(&dir),
            Err(_) => PathBuf::from(Self::STATE_DIRECTORY_DEFAULT_VALUE),
        };

        // Make sure the state directory exists
        match std::fs::create_dir_all(&state_directory) {
            // Directory created successfully
            Ok(_) => println!("State directory created: {}", state_directory.display()),

            // Directory already exists, no action needed
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),

            // All Other errors
            Err(e) => return Err(BotError::new(format!("Failed to create state directory: {}", e))),
        }

        // Canonicalize the state directory path
        let state_directory = match state_directory.canonicalize() {
            Ok(path) => path,
            Err(e) => return Err(BotError::new(format!("Failed to get absolute path to state directory: {}", e))),
        };

        // Get the Database Connection String, and configure the database options accordingly
        let mut database_options = sqlx::sqlite::SqliteConnectOptions::new().create_if_missing(true);
        let database_url = match std::env::var(Self::DB_CONNECTION_STRING_ENV_VAR) {
            Ok(url) => match url.strip_prefix("sqlite:") {
                Some(path) => {
                    // If the URL starts with "sqlite:", we strip off the prefix and use the remaining part as the file path
                    database_options = database_options.filename(path);
                    url
                },
                None => {
                    // If the URL does not start with "sqlite:", take it directly as the file path and prepend "sqlite:" to it
                    database_options = database_options.filename(url.clone());
                    format!("sqlite:{}", url)
                },
            },
            Err(e) => {
                return Err(BotError::new(format!("Environment variable {} is not set: {}", Self::DB_CONNECTION_STRING_ENV_VAR, e)));
            },
        };

        // Create a connection pool to the SQLite database using the configured options
        let database = match sqlx::sqlite::SqlitePoolOptions::new().connect_with(database_options).await {
            Ok(pool) => pool,
            Err(e) => return Err(BotError::new(format!("Failed to connect to database: {}", e))),
        };

        // Run Migrations
        if let Err(e) = sqlx::migrate!().run(&database).await {
            return Err(BotError::new(format!("Failed to run database migrations: {}", e)));
        }

        Ok(CalendarBot { token, state_directory, database_url, database })
    }

    // Run the bot, this will block until the bot is stopped
    pub async fn run(self) -> Result<(), BotError> {
        // Combine the intents into a single GatewayIntents value
        let mut intents = GatewayIntents::empty();
        for intent in Self::INTENTS {
            intents |= intent;
        }

        // Create the Serenity client
        let mut client = match serenity::Client::builder(&self.token, intents).event_handler(self).await {
            Ok(client) => client,
            Err(e) => return Err(BotError::new(format!("Err creating client: {}", e))),
        };

        // Start the client, this will block until the bot is stopped
        if let Err(e) = client.start().await {
            return Err(BotError::new(format!("Failed to start the bot: {}", e)));
        }
        Ok(())
    }
}


#[async_trait]
impl EventHandler for CalendarBot {
    async fn message(&self, ctx: Context, msg: Message) {
        let channel_name = match msg.channel(&ctx).await {
            Ok(channel) => match channel.guild() {
                Some(guild_channel) => format!("{}#{}", guild_channel.guild_id, guild_channel.name),
                None => "DM".to_string(),
            },
            Err(_) => msg.channel_id.to_string(),
        };

        println!("{}/{} : {} --> {}", channel_name, msg.id, msg.author.name, msg.content);
    }
}
