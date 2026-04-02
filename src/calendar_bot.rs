use std::path::PathBuf;

use chrono::SecondsFormat;
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


pub fn format_timestamp(timestamp: &Timestamp) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Secs, false)
}

#[derive(Debug, Clone)]
pub struct EventDetails {
    _event: ScheduledEvent,
}

impl From<ScheduledEvent> for EventDetails {
    fn from(event: ScheduledEvent) -> EventDetails {
        EventDetails::new(event)
    }
}

impl EventDetails {
    pub fn new(event: ScheduledEvent) -> Self {
        Self { _event: event }
    }

    pub fn event_id(&self) -> String {
        self._event.id.get().to_string()
    }

    fn name(&self) -> String {
        self._event.name.clone()
    }

    fn description(&self) -> Option<String> {
        self._event.description.clone()
    }

    fn location(&self) -> Option<String> {
        self._event.metadata.clone().and_then(|meta| meta.location.clone())
    }

    fn start_time(&self) -> String {
        format_timestamp(&self._event.start_time)
    }

    fn end_time(&self) -> Option<String> {
        self._event.end_time.map(|end_time| format_timestamp(&end_time))
    }

    fn duration_seconds(&self) -> i64 {
        match self._event.end_time {
            Some(end_time) => {
                end_time.timestamp() - self._event.start_time.timestamp()
            },
            // If no end time is provided, treat it as a 0-second event
            None => 0,
        }
    }
}


#[async_trait]
impl EventHandler for CalendarBot {
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected and waiting for events!", ready.user.name);
    }

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

    async fn guild_scheduled_event_create(&self, _ctx: Context, event: ScheduledEvent) {
        println!("New Scheduled Event: {} (ID: {})", event.name, event.id);
        let event_details = EventDetails::from(event);

        match sqlx::query!(
            r#"
            INSERT INTO events (
                discord_event_id,
                name,
                description,
                location,
                start_time,
                duration_seconds,
                created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
            "#,
            event_details.event_id(),
            event_details.name(),
            event_details.description(),
            event_details.location(),
            event_details.start_time(),
            event_details.duration_seconds(),
        ).execute(&self.database).await {
            Ok(_) => println!("Event inserted into database successfully."),
            Err(e) => eprintln!("Failed to insert event into database: {}", e),
        };

    }

    async fn guild_scheduled_event_update(&self, _ctx: Context, event: ScheduledEvent) {
        println!("Scheduled Event Updated: {} (ID: {})", event.name, event.id);

        // Update the event in the database
        let event_id = event.id.get().to_string();
        let location = event.metadata.and_then(|meta| meta.location).to_owned();
        let event_start = event.start_time.to_rfc3339().unwrap_or_else(|| Timestamp::now().to_rfc3339().unwrap());
        let duration_seconds = match event.end_time {
            Some(end_time) => {
                let start_time = event.start_time;
                end_time.timestamp() - start_time.timestamp()
            },
            // If no end time is provided, treat it as a 0-second event
            None => 0,
        };

        match sqlx::query!(
            r#"
            UPDATE events
            SET
                name = ?,
                description = ?,
                location = ?,
                start_time = ?,
                duration_seconds = ?
            WHERE discord_event_id = ?
            "#,
            event.name,
            event.description,
            location,
            event_start,
            duration_seconds,
            event_id,
        ).execute(&self.database).await {
            Ok(_) => println!("Event updated in database successfully."),
            Err(e) => eprintln!("Failed to update event in database: {}", e),
        };

    }

    async fn guild_scheduled_event_delete(&self, _ctx: Context, event: ScheduledEvent) {
        println!("Scheduled Event Deleted: {} (ID: {})", event.name, event.id);

        // Mark the event as deleted in the database
        let event_id = event.id.get().to_string();
        match sqlx::query!(
            r#"
            UPDATE events
            SET deleted = 1
            WHERE discord_event_id = ?
            "#,
            event_id,
        ).execute(&self.database).await {
            Ok(_) => println!("Event marked as deleted in database successfully."),
            Err(e) => eprintln!("Failed to mark event as deleted in database: {}", e),
        };
    }
}
