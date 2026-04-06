use rand::Rng;

use crate::errors::BotError;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct EventDetails
{
    pub event_id: String,
    pub guild_id: String,
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub deleted: bool,
    pub sequence: i64,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub end_time: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub trait Crud<T>
where
    T: Sync + Clone + Eq,
{
    async fn lookup(&self, db: &sqlx::SqlitePool) -> Result<T, BotError>;
    async fn refresh(&mut self, db: &sqlx::SqlitePool, check_only: bool) -> Result<bool, BotError>;
    async fn create(&self, db: &sqlx::SqlitePool) -> Option<BotError>;
    async fn update(&self, db: &sqlx::SqlitePool) -> Option<BotError>;
    async fn delete(&self, db: &sqlx::SqlitePool) -> Option<BotError>;
}

impl Crud<EventDetails> for EventDetails
{
    async fn lookup(&self, db: &sqlx::SqlitePool) -> Result<EventDetails, BotError>
    {
        sqlx::query_as("SELECT * FROM events WHERE event_id = ?")
            .bind(self.event_id.clone())
            .fetch_one(db)
            .await
            .map_err(|e| BotError::new(format!("FAILED to Lookup EventDetails due to: {}", e)))
    }

    /// Update the created/updated date on the struct from the DB,
    /// and return whether the other fields have changed compared to the DB.
    /// If `check_only` is false, also update the other fields to match the DB.
    async fn refresh(&mut self, db: &sqlx::SqlitePool, check_only: bool) -> Result<bool, BotError>
    {
        // Lookup the version of us in the DB
        let version_in_db: EventDetails = self.lookup(db).await?;

        // Copy the created and updated dates from DB to us since the ones
        // in the DB are always authoratative when we are refreshing
        self.created_at = version_in_db.created_at;
        self.updated_at = version_in_db.updated_at;

        // Check for differences in our other fields
        let has_changed = !version_in_db.eq(self);

        if !check_only && has_changed {
            // Update ourselves
            version_in_db.clone_into(self);
        }

        Ok(has_changed)
    }

    async fn create(&self, db: &sqlx::SqlitePool) -> Option<BotError>
    {
        sqlx::query!(
            r#"
            INSERT INTO events (
                event_id,
                guild_id,
                title,
                description,
                location,
                deleted,
                sequence,
                start_time,
                end_time,
                created_at,
                updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            self.event_id,
            self.guild_id,
            self.title,
            self.description,
            self.location,
            self.deleted,
            self.sequence,
            self.start_time,
            self.end_time,
            self.created_at,
            self.updated_at,
        )
        .execute(db)
        .await
        .map_err(|e| BotError::new(format!("FAILED to Insert EventDetails due to: {}", e)))
        .err()
    }

    async fn update(&self, db: &sqlx::SqlitePool) -> Option<BotError>
    {
        let now = chrono::Utc::now();
        sqlx::query!(
            r#"
            UPDATE events SET
                title = ?,
                description = ?,
                location = ?,
                deleted = ?,
                sequence = ?,
                start_time = ?,
                end_time = ?,
                updated_at = ?
            WHERE
                event_id = ?
            "#,
            self.title,
            self.description,
            self.location,
            self.deleted,
            self.sequence,
            self.start_time,
            self.end_time,
            now,
            self.event_id,
        )
        .execute(db)
        .await
        .map_err(|e| BotError::new(format!("FAILED to Update EventDetails due to: {}", e)))
        .err()
    }

    async fn delete(&self, db: &sqlx::SqlitePool) -> Option<BotError>
    {
        sqlx::query!("UPDATE events SET deleted = 1 WHERE event_id = ? ", self.event_id,)
            .execute(db)
            .await
            .map_err(|e| BotError::new(format!("FAILED to Delete EventDetails due to: {}", e)))
            .err()
    }
}

impl From<serenity::all::ScheduledEvent> for EventDetails
{
    fn from(value: serenity::all::ScheduledEvent) -> Self
    {
        let guild_id = value.guild_id.get().to_string();
        let event_id = value.id.get().to_string();
        let now = chrono::Utc::now();

        let start_time = value.start_time.to_utc();
        let end_time = match value.end_time {
            Some(time) => time.to_utc(),
            None => start_time.clone(),
        };

        Self {
            guild_id,
            event_id,
            start_time,
            end_time,
            title: value.name,
            description: value.description,
            location: value.metadata.and_then(|meta| meta.location),
            created_at: now,
            updated_at: now,
            deleted: false,
            sequence: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct GuildRecord
{
    pub guild_id: String,
    pub name: String,
    pub discriminator: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl GuildRecord
{
    pub async fn upsert(&self, db: &sqlx::SqlitePool) -> Option<BotError>
    {
        let now = chrono::Utc::now();
        sqlx::query!(
            r#"
            INSERT INTO guilds (guild_id, name, created_at, updated_at)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(guild_id) DO UPDATE SET
                name = excluded.name,
                updated_at = ?
            "#,
            self.guild_id,
            self.name,
            now,
            now,
            now,
        )
        .execute(db)
        .await
        .map_err(|e| BotError::new(format!("FAILED to Upsert GuildInfo due to: {}", e)))
        .err()
    }
}

impl From<&serenity::all::Guild> for GuildRecord
{
    fn from(value: &serenity::all::Guild) -> Self
    {
        let now = chrono::Utc::now();
        Self {
            guild_id: value.id.get().to_string(),
            name: value.name.clone(),
            discriminator: None,
            created_at: now,
            updated_at: now,
        }
    }
}

impl GuildRecord
{
    pub fn from_id(guild_id: serenity::all::GuildId) -> Self
    {
        let now = chrono::Utc::now();
        Self {
            guild_id: guild_id.get().to_string(),
            name: String::new(),
            discriminator: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Generate a random 32-character hex string.
    pub fn generate_discriminator() -> String
    {
        let bytes: [u8; 16] = rand::rng().random();
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Set the discriminator for a guild in the database.
    pub async fn set_discriminator(
        guild_id: &str,
        discriminator: &str,
        db: &sqlx::SqlitePool,
    ) -> Option<BotError>
    {
        let now = chrono::Utc::now();
        sqlx::query!(
            r#"
            UPDATE guilds SET
                discriminator = ?,
                updated_at = ?
            WHERE guild_id = ?
            "#,
            discriminator,
            now,
            guild_id,
        )
        .execute(db)
        .await
        .map_err(|e| BotError::new(format!("FAILED to set discriminator: {}", e)))
        .err()
    }

    /// Clear (remove) the discriminator for a guild in the database.
    pub async fn clear_discriminator(guild_id: &str, db: &sqlx::SqlitePool) -> Option<BotError>
    {
        let now = chrono::Utc::now();
        let none: Option<&str> = None;
        sqlx::query!(
            r#"
            UPDATE guilds SET
                discriminator = ?,
                updated_at = ?
            WHERE guild_id = ?
            "#,
            none,
            now,
            guild_id,
        )
        .execute(db)
        .await
        .map_err(|e| BotError::new(format!("FAILED to clear discriminator: {}", e)))
        .err()
    }

    /// Look up the guild record by guild_id.
    pub async fn lookup(guild_id: &str, db: &sqlx::SqlitePool) -> Result<Option<GuildRecord>, BotError>
    {
        sqlx::query_as("SELECT * FROM guilds WHERE guild_id = ?")
            .bind(guild_id)
            .fetch_optional(db)
            .await
            .map_err(|e| BotError::new(format!("FAILED to lookup guild: {}", e)))
    }

    /// Returns true if the given discriminator is URL-safe
    /// (contains only unreserved URI characters: A-Z, a-z, 0-9, -, ., _, ~).
    pub fn is_url_safe(discriminator: &str) -> bool
    {
        !discriminator.is_empty()
            && discriminator
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_' || c == '~')
    }
}
