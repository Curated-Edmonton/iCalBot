use crate::errors::BotError;

#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(sqlx::FromRow)]
pub struct EventDetails {
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

pub trait Crud<T> where T : Sync + Clone + Eq {
    async fn lookup(&self, db: &sqlx::SqlitePool)  -> Result<T, BotError>;
    async fn refresh(&mut self, db: &sqlx::SqlitePool, check_only: bool)  -> Result<bool, BotError>;
    async fn create(&self, db: &sqlx::SqlitePool)  -> Option<BotError>;
    async fn update(&self, db: &sqlx::SqlitePool)  -> Option<BotError>;
    async fn delete(&self, db: &sqlx::SqlitePool)  -> Option<BotError>;
}

impl Crud<EventDetails> for EventDetails {

    async fn lookup(&self, db: &sqlx::SqlitePool)  -> Result<EventDetails, BotError> {
        sqlx::query_as(
            "SELECT * FROM events WHERE event_id = ?"
        ).bind(self.event_id.clone()).fetch_one(db).await.map_err(
            |e| BotError::new(format!("FAILED to Lookup EventDetails due to: {}", e))
        )
    }

    /// Update the created/updated date on the struct from the DB,
    /// and return whether the other fields have changed compared to the DB.
    /// If `check_only` is false, also update the other fields to match the DB.
    async fn refresh(&mut self, db: &sqlx::SqlitePool, check_only: bool)  -> Result<bool, BotError> {
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
        ).execute(db).await.map_err(
            |e| BotError::new(format!("FAILED to Insert EventDetails due to: {}", e))
        ).err()
    }

    async fn update(&self, db: &sqlx::SqlitePool)  -> Option<BotError> {
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
        ).execute(db).await.map_err(
            |e| BotError::new(format!("FAILED to Update EventDetails due to: {}", e))
        ).err()
    }

    async fn delete(&self, db: &sqlx::SqlitePool)  -> Option<BotError> {
        sqlx::query!(
            "UPDATE events SET deleted = 1 WHERE event_id = ? ",
            self.event_id,
        ).execute(db).await.map_err(
            |e| BotError::new(format!("FAILED to Delete EventDetails due to: {}", e))
        ).err()
    }
}

impl From<serenity::all::ScheduledEvent> for EventDetails {
    fn from(value: serenity::all::ScheduledEvent) -> Self {
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

