use std::path::PathBuf;

use serenity::async_trait;
use serenity::model::prelude::*;
use serenity::prelude::*;

use crate::entities::*;
use crate::errors::BotError;
use crate::icalendar::make_calendar;

// Define a struct to hold the global state of the bot
#[allow(unused)]
pub struct CalendarBot {
    client: Option<Client>,
    pub version: String,
    pub token: String,
    pub state_directory: PathBuf,
    pub database_url: String,
    pub database: sqlx::SqlitePool,
}

#[cfg(unix)]
async fn wait_until_shutdown() {
    use tokio::signal::unix as signal;

    let [mut s1, mut s2, mut s3] = [
        signal::signal(signal::SignalKind::hangup()).unwrap(),
        signal::signal(signal::SignalKind::interrupt()).unwrap(),
        signal::signal(signal::SignalKind::terminate()).unwrap(),
    ];

    tokio::select!(
        v = s1.recv() => v.unwrap(),
        v = s2.recv() => v.unwrap(),
        v = s3.recv() => v.unwrap(),
    );
}

#[cfg(windows)]
async fn wait_until_shutdown() {
    let (mut s1, mut s2) = (
        tokio::signal::windows::ctrl_c().unwrap(),
        tokio::signal::windows::ctrl_break().unwrap(),
    );

    tokio::select!(
        v = s1.recv() => v.unwrap(),
        v = s2.recv() => v.unwrap(),
    );
}

// And also act as a Type on which to implement Traits
impl CalendarBot {
    const BOT_TOKEN_ENV_VAR: &str = "BOT_TOKEN";

    // Either a file path to a SQLite database or a full connection string starting with "sqlite:"
    const DB_CONNECTION_STRING_ENV_VAR: &str = "DATABASE_URL";
    const DB_CONNECTION_STRING_DEFAULT_FILENAME: &str = "db.sqlite";

    const STATE_DIRECTORY_ENV_VAR: &str = "BOT_STATE_DIRECTORY";
    const STATE_DIRECTORY_DEFAULT_VALUE: &str = "icalbot";

    const INTENTS: [GatewayIntents; 2] = [
        GatewayIntents::GUILD_SCHEDULED_EVENTS,
        GatewayIntents::MESSAGE_CONTENT,
    ];

    const BOT_COMMAND_PREFIX: &str = "!icalbot ";

    // Initialize the bot
    pub async fn new() -> Result<Self, BotError> {
        // Set the Version from BUILD_VERSION variable which should be setup by build.rs
        let version = env!("BUILD_VERSION").to_owned();
        println!("Using Version: {}", version);

        // Get the Bot Token
        let token = match std::env::var(Self::BOT_TOKEN_ENV_VAR) {
            Ok(token) => token,
            Err(_) => {
                return Err(BotError::new(format!(
                    "Environment variable {} is not set",
                    Self::BOT_TOKEN_ENV_VAR
                )));
            }
        };

        // Get the State Directory as an absolute path
        let state_directory = match std::env::var(Self::STATE_DIRECTORY_ENV_VAR) {
            Ok(dir) => PathBuf::from(&dir),
            Err(_) => PathBuf::from(Self::STATE_DIRECTORY_DEFAULT_VALUE),
        };

        // Make sure the state directory exists
        match std::fs::create_dir_all(&state_directory) {
            // Directory created successfully
            Ok(_) => println!("Created new empty state directory."),

            // Directory already exists, no action needed
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),

            // All Other errors
            Err(e) => {
                return Err(BotError::new(format!(
                    "Failed to create state directory: {}",
                    e
                )));
            }
        }

        // Canonicalize the state directory path
        let state_directory = match state_directory.canonicalize() {
            Ok(path) => path,
            Err(e) => {
                return Err(BotError::new(format!(
                    "Failed to get absolute path to state directory: {}",
                    e
                )));
            }
        };

        println!("Using state directory: {}", state_directory.display());

        // Get the Database Connection String, and configure the database options accordingly
        let mut database_options =
            sqlx::sqlite::SqliteConnectOptions::new().create_if_missing(true);
        let database_url = match std::env::var(Self::DB_CONNECTION_STRING_ENV_VAR) {
            Ok(url) => match url.strip_prefix("sqlite:") {
                Some(path) => {
                    // If the URL starts with "sqlite:", we strip off the prefix and use the remaining part as the file path
                    database_options = database_options.filename(path);
                    url
                }
                None => {
                    // If the URL does not start with "sqlite:", take it directly as the file path and prepend "sqlite:" to it
                    database_options = database_options.filename(url.clone());
                    format!("sqlite:{}", url)
                }
            },
            Err(std::env::VarError::NotPresent) => {
                // If the environment variable is not set, use a default file path in the state directory
                let default_path =
                    state_directory.join(Self::DB_CONNECTION_STRING_DEFAULT_FILENAME);
                database_options = database_options.filename(&default_path);
                format!("sqlite:{}", default_path.display())
            }
            Err(e) => {
                return Err(BotError::new(format!(
                    "Error while getting environment variable {}: {}",
                    Self::DB_CONNECTION_STRING_ENV_VAR,
                    e
                )));
            }
        };

        println!("Using database connection string: {}", database_url);

        // Create a connection pool to the SQLite database using the configured options
        let database = match sqlx::sqlite::SqlitePoolOptions::new()
            .connect_with(database_options)
            .await
        {
            Ok(pool) => pool,
            Err(e) => {
                return Err(BotError::new(format!(
                    "Failed to connect to database: {}",
                    e
                )));
            }
        };

        // Run Migrations
        if let Err(e) = sqlx::migrate!().run(&database).await {
            return Err(BotError::new(format!(
                "Failed to run database migrations: {}",
                e
            )));
        }

        // Return the constructed bot
        Ok(CalendarBot {
            client: None,
            version,
            token,
            state_directory,
            database_url,
            database,
        })
    }

    // Run the bot, this will block until the bot is stopped
    pub async fn run(self) -> Result<(), BotError> {
        // Combine the intents into a single GatewayIntents value
        let mut intents = GatewayIntents::default();
        for intent in Self::INTENTS {
            intents |= intent;
        }

        // Create the Serenity client
        let mut client = match serenity::Client::builder(&self.token, intents)
            .event_handler(self)
            .await
        {
            Ok(client) => client,
            Err(e) => return Err(BotError::new(format!("Err creating client: {}", e))),
        };

        let shard_manager = client.shard_manager.clone();
        tokio::spawn(async move {
            wait_until_shutdown().await;
            println!("Recieved control C and shutting down.");
            shard_manager.shutdown_all().await;
        });

        // Start the client, this will block until the bot is stopped
        if let Err(e) = client.start_autosharded().await {
            return Err(BotError::new(format!("Failed to start the bot: {}", e)));
        }
        Ok(())
    }

    /// Given a Guild ID, iterate all accessible events and update them in the DB
    async fn refresh_guild_events(&self, ctx: Context, guild: GuildId) {
        let guild_id = guild.get().to_string();
        let guild_name = guild.name(ctx.cache).unwrap_or(guild_id);

        // Get the list of scheduled events for the guild
        let Ok(guild_events) = guild.scheduled_events(&ctx.http, true).await else {
            println!("Unable to get scheduled events.");
            return;
        };

        if guild_events.len() == 0 {
            println!(
                "[upsert_guild_events] Guild {} has no visible event history.",
                guild_name
            );
            return;
        }

        // Refresh each event and count the number of changes
        let mut num_events_different: u16 = 0;
        for mut event in guild_events.iter().map(|e| EventDetails::from(e.clone())) {
            if event.refresh(&self.database, false).await.unwrap_or(false) {
                num_events_different += 1;
            }
        }

        println!(
            "[refresh_guild_events] {}: Events Refreshed {}/{} were changed.",
            guild_name,
            num_events_different,
            guild_events.len()
        )
    }
}

#[async_trait]
impl EventHandler for CalendarBot {
    async fn ready(&self, _ctx: Context, ready: Ready) {
        println!("{} is connected and ready!", ready.user.name);
    }

    async fn guild_create(&self, ctx: Context, guild: Guild, is_new: Option<bool>) {
        println!(
            "[guild_create] {} ({}) (is_new: {:?})",
            guild.name,
            guild.id.get(),
            is_new
        );
        self.refresh_guild_events(ctx, guild.id).await;
    }

    async fn message(&self, ctx: Context, msg: Message) {
        let Some(guild_id) = msg.guild_id else {
            // Return early if the message has no guild
            return;
        };

        // Dispatch based on the command
        match msg.content.strip_prefix(CalendarBot::BOT_COMMAND_PREFIX) {
            Some("refresh") => self.refresh_guild_events(ctx, guild_id).await,
            Some("version") => _ = msg.reply_mention(ctx, self.version.clone()).await,
            Some("dump_calendar") => match make_calendar(guild_id, &ctx, &self.database).await {
                Ok(cal) => {
                    _ = msg
                        .reply_mention(ctx, format!("```plaintext\n{}\n```", cal))
                        .await
                }
                Err(e) => _ = msg
                    .reply_mention(
                        ctx,
                        format!(
                            "I had an error rendering your calendar...\n\n```plaintext\n{}\n```",
                            e
                        ),
                    )
                    .await,
            },
            Some(unknown) => println!("Unknown Command: {unknown}"),
            None => return,
        }
    }

    async fn guild_scheduled_event_create(&self, ctx: Context, event: ScheduledEvent) {
        print!(
            "[guild_scheduled_event_create]: {:?}/{} ",
            event.guild_id.name(&ctx.cache),
            event.name
        );
        let event_details: EventDetails = event.into();
        event_details.create(&self.database).await;
        println!("Success");
    }

    async fn guild_scheduled_event_update(&self, ctx: Context, event: ScheduledEvent) {
        print!(
            "[guild_scheduled_event_update]: {:?}/{} ",
            event.guild_id.name(&ctx.cache),
            event.name
        );
        let event_details: EventDetails = event.into();
        event_details.update(&self.database).await;
        println!("Success");
    }

    async fn guild_scheduled_event_delete(&self, ctx: Context, event: ScheduledEvent) {
        print!(
            "[guild_scheduled_event_delete]: {:?}/{} ",
            event.guild_id.name(&ctx.cache),
            event.name
        );
        let event_details: EventDetails = event.into();
        event_details.delete(&self.database).await;
        println!("Success");
    }
}
