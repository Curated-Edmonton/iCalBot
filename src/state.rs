use std::path::PathBuf;

use crate::errors::BotError;

pub struct AppState
{
    pub state_directory: PathBuf,
    pub database_url: String,
    pub database: sqlx::SqlitePool,
    pub base_url: String,
}

impl AppState
{
    const DB_CONNECTION_STRING_ENV_VAR: &str = "DATABASE_URL";
    const DB_CONNECTION_STRING_DEFAULT_FILENAME: &str = "db.sqlite";

    const STATE_DIRECTORY_ENV_VAR: &str = "BOT_STATE_DIRECTORY";
    const STATE_DIRECTORY_DEFAULT_VALUE: &str = "icalbot";

    const LISTEN_ADDRESS_ENV_VAR: &str = "WEB_SERVER_LISTEN_ADDRESS";
    const LISTEN_ADDRESS_DEFAULT: &str = "0.0.0.0";

    const LISTEN_PORT_ENV_VAR: &str = "WEB_SERVER_LISTEN_PORT";
    const LISTEN_PORT_DEFAULT: &str = "29273";

    const WEB_SERVER_BASE_URL_ENV_VAR: &str = "WEB_SERVER_BASE_URL";

    pub async fn new() -> Result<Self, BotError>
    {
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
                return Err(BotError::new(format!("Failed to create state directory: {}", e)));
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
        let mut database_options = sqlx::sqlite::SqliteConnectOptions::new().create_if_missing(true);
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
                let default_path = state_directory.join(Self::DB_CONNECTION_STRING_DEFAULT_FILENAME);
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
                return Err(BotError::new(format!("Failed to connect to database: {}", e)));
            }
        };

        // Run Migrations
        if let Err(e) = sqlx::migrate!().run(&database).await {
            return Err(BotError::new(format!("Failed to run database migrations: {}", e)));
        }

        // Compute the base URL from environment or defaults.
        let listen_host = std::env::var(Self::LISTEN_ADDRESS_ENV_VAR)
            .unwrap_or_else(|_| Self::LISTEN_ADDRESS_DEFAULT.into());
        let listen_port =
            std::env::var(Self::LISTEN_PORT_ENV_VAR).unwrap_or_else(|_| Self::LISTEN_PORT_DEFAULT.into());
        let base_url = std::env::var(Self::WEB_SERVER_BASE_URL_ENV_VAR)
            .unwrap_or_else(|_| format!("{}:{}", listen_host, listen_port));
        let base_url = base_url.trim_end_matches('/').to_string();

        println!("Using base URL: {}", base_url);

        Ok(Self {
            state_directory,
            database_url,
            database,
            base_url,
        })
    }

    /// Return the listen address (host:port) derived from environment or defaults.
    pub fn listen_address(&self) -> String
    {
        let listen_host = std::env::var(Self::LISTEN_ADDRESS_ENV_VAR)
            .unwrap_or_else(|_| Self::LISTEN_ADDRESS_DEFAULT.into());
        let listen_port =
            std::env::var(Self::LISTEN_PORT_ENV_VAR).unwrap_or_else(|_| Self::LISTEN_PORT_DEFAULT.into());
        format!("{}:{}", listen_host, listen_port)
    }

    /// Build the full calendar URL for a guild given its discriminator.
    pub fn calendar_url(&self, discriminator: &str) -> String
    {
        format!("{}/ical/{}/ical.ics", self.base_url, discriminator)
    }
}
