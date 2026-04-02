use std::{error::Error, path::PathBuf};
use dotenv::dotenv;
use sqlx;

// Define a custom error type for the bot
#[derive(Debug, Clone)]
struct BotError {
    message: String,
}

impl BotError {
    fn new(message: String) -> Self {
        BotError { message }
    }
}

impl Error for BotError {}

impl std::fmt::Display for BotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

// Define a struct to hold the global state of the bot
struct CalendarBot {
    token: String,
    state_directory: PathBuf,
    database: sqlx::SqlitePool,
}


// And also act as a Type on which to implement Traits
impl CalendarBot {
    const BOT_TOKEN_ENV_VAR: &str = "BOT_TOKEN";

    const STATE_DIRECTORY_ENV_VAR: &str = "BOT_STATE_DIRECTORY";
    const STATE_DIRECTORY_DEFAULT_VALUE: &str = "./discordcalendarbot";

    // Default to in memory database if no connection string is provided
    const DB_CONNECTION_STRING_ENV_VAR: &str = "DATABASE_URL";
    const DB_CONNECTION_STRING_DEFAULT_VALUE: &str = "sqlite::memory:";

    async fn new() -> Result<Self, BotError> {
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

        // Get the Database Connection String
        let conn_str = match std::env::var(Self::DB_CONNECTION_STRING_ENV_VAR) {
            Ok(conn_str) => conn_str,
            Err(_) => Self::DB_CONNECTION_STRING_DEFAULT_VALUE.to_string(),
        };

        // Setup Database Connection
        let database = match sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(conn_str)
                .create_if_missing(true),
        ).await {
            Ok(pool) => pool,
            Err(e) => return Err(BotError::new(format!("Failed to connect to database: {}", e))),
        };

        // Run Migrations
        if let Err(e) = sqlx::migrate!().run(&database).await {
            return Err(BotError::new(format!("Failed to run database migrations: {}", e)));
        }

        Ok(CalendarBot { token, state_directory, database })
    }
}

#[tokio::main]
async fn main() {
    // Load environment variables from a .env file, if it exists
    dotenv().ok();

    // Create the bot instance
    let _bot = CalendarBot::new().await
        .expect("Failed to initialize the bot");
}
