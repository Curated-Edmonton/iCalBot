use std::error::Error;
use dotenv::dotenv;
use sqlx;
use sqlx::SqlitePool;

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
    state_directory: String,
    database: SqlitePool,
}


// And also act as a Type on which to implement Traits
impl CalendarBot {
    const BOT_TOKEN_ENV_VAR: &str = "BOT_TOKEN";
    const STATE_DIRECTORY_ENV_VAR: &str = "BOT_STATE_DIRECTORY";
    const STATE_DIRECTORY_DEFAULT_VALUE: &str = "./discordcalendarbot";

    async fn new() -> Result<Self, BotError> {
        // Get the Bot Token
        let token = match std::env::var(Self::BOT_TOKEN_ENV_VAR) {
            Ok(token) => token,
            Err(_) => return Err(BotError::new(format!("Environment variable {} is not set", Self::BOT_TOKEN_ENV_VAR))),
        };

        // Get the State Directory
        let state_directory = match std::env::var(Self::STATE_DIRECTORY_ENV_VAR) {
            Ok(dir) => dir,
            Err(_) => Self::STATE_DIRECTORY_DEFAULT_VALUE.to_string(),
        };

        // Make sure the state directory exists
        match std::fs::create_dir_all(&state_directory) {
            // Directory created successfully
            Ok(_) => println!("State directory created: {}", state_directory),

            // Directory already exists, no action needed
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),

            // All Other errors
            Err(e) => return Err(BotError::new(format!("Failed to create state directory: {}", e))),
        }

        // Connect to the database
        let conn_str = format!("sqlite:{}", state_directory.clone() + "/calendar_bot.sqlite");
        let database = match SqlitePool::connect(&conn_str).await {
            Ok(pool) => pool,
            Err(e) => return Err(BotError::new(format!("Failed to connect to the database: {}", e))),
        };

        // Run Migrations
        // if let Err(e) = sqlx::migrate!("./migrations").run(&database).await {
        //     return Err(BotError::new(format!("Failed to run database migrations: {}", e))));
        // }

        Ok(CalendarBot { token, state_directory, database })
    }
}

#[tokio::main]
async fn main() {
    // Load environment variables from a .env file, if it exists
    dotenv().ok();

    // Initialize the bot's state
    let _bot = CalendarBot::new().await
        .expect("Failed to initialize the bot");

    println!("Hello, world!");
}
