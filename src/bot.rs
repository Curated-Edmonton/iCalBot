use std::sync::Arc;

use serenity::async_trait;
use serenity::model::prelude::*;
use serenity::prelude::*;
use tokio::sync::watch;

use crate::entities::*;
use crate::errors::BotError;
use crate::icalendar::make_calendar;
use crate::state::AppState;

struct RefreshResult
{
    total_events: i64,
    past_events: i64,
    future_events: i64,
    refreshed_events: u16,
}

// Define a struct to hold the global state of the bot
#[allow(unused)]
pub struct CalendarBot
{
    client: Option<Client>,
    pub version: String,
    pub token: String,
    pub state: Arc<AppState>,
}

// And also act as a Type on which to implement Traits
impl CalendarBot
{
    const BOT_TOKEN_ENV_VAR: &str = "BOT_TOKEN";

    const INTENTS: [GatewayIntents; 2] = [
        GatewayIntents::GUILD_SCHEDULED_EVENTS,
        GatewayIntents::MESSAGE_CONTENT,
    ];

    const BOT_COMMAND_PREFIX: &str = "!icalbot";

    // Initialize the bot
    pub async fn new(state: Arc<AppState>) -> Result<Self, BotError>
    {
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

        // Return the constructed bot
        Ok(CalendarBot {
            client: None,
            version,
            token,
            state,
        })
    }

    // Run the bot, this will block until the bot is stopped
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) -> Result<(), BotError>
    {
        println!("Starting Discord Bot.");

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
            while shutdown.changed().await.is_ok() {
                if *shutdown.borrow() {
                    println!("Shutdown requested. Stopping Discord bot shards.");
                    shard_manager.shutdown_all().await;
                    break;
                }
            }
        });

        // Start the client, this will block until the bot is stopped
        if let Err(e) = client.start_autosharded().await {
            return Err(BotError::new(format!("Failed to start the bot: {}", e)));
        }
        Ok(())
    }

    /// Given a Guild ID, iterate all accessible events and update them in the DB
    async fn refresh_guild_events(&self, ctx: Context, guild: GuildId) -> Option<RefreshResult>
    {
        let guild_id_str = guild.get().to_string();
        let guild_name = guild.name(ctx.cache).unwrap_or(guild_id_str.clone());

        // Get the list of scheduled events for the guild
        let Ok(guild_events) = guild.scheduled_events(&ctx.http, true).await else {
            println!("Unable to get scheduled events.");
            return None;
        };

        if guild_events.len() == 0 {
            println!(
                "[upsert_guild_events] Guild {} has no visible event history.",
                guild_name
            );
        }

        // Refresh each event and count the number of changes
        let mut num_events_different: u16 = 0;
        for mut event in guild_events.iter().map(|e| EventDetails::from(e.clone())) {
            if event.refresh(&self.state.database, false).await.unwrap_or(false) {
                num_events_different += 1;
            }
        }

        println!(
            "[refresh_guild_events] {}: Events Refreshed {}/{} were changed.",
            guild_name,
            num_events_different,
            guild_events.len()
        );

        // Query the database for event counts
        let now = chrono::Utc::now();
        let total_events: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE guild_id = ? AND deleted = 0")
                .bind(&guild_id_str)
                .fetch_one(&self.state.database)
                .await
                .unwrap_or(0);

        let past_events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM events WHERE guild_id = ? AND deleted = 0 AND end_time < ?",
        )
        .bind(&guild_id_str)
        .bind(now)
        .fetch_one(&self.state.database)
        .await
        .unwrap_or(0);

        let future_events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM events WHERE guild_id = ? AND deleted = 0 AND start_time >= ?",
        )
        .bind(&guild_id_str)
        .bind(now)
        .fetch_one(&self.state.database)
        .await
        .unwrap_or(0);

        Some(RefreshResult {
            total_events,
            past_events,
            future_events,
            refreshed_events: num_events_different,
        })
    }
}

#[async_trait]
impl EventHandler for CalendarBot
{
    async fn ready(&self, _ctx: Context, ready: Ready)
    {
        println!("{} is connected and ready!", ready.user.name);

        for guild in &ready.guilds {
            let guild_info = GuildRecord::from_id(guild.id);
            if let Some(err) = guild_info.upsert(&self.state.database).await {
                println!("[ready] Failed to upsert guild {}: {}", guild.id.get(), err);
            }
        }
        println!("[ready] Upserted {} guild(s).", ready.guilds.len());
    }

    async fn guild_create(&self, ctx: Context, guild: Guild, is_new: Option<bool>)
    {
        println!(
            "[guild_create] {} ({}) (is_new: {:?})",
            guild.name,
            guild.id.get(),
            is_new
        );

        let guild_info = GuildRecord::from(&guild);
        if let Some(err) = guild_info.upsert(&self.state.database).await {
            println!("[guild_create] Failed to upsert guild {}: {}", guild.name, err);
        }

        self.refresh_guild_events(ctx, guild.id).await;
    }

    async fn message(&self, ctx: Context, msg: Message)
    {
        let Some(guild_id) = msg.guild_id else {
            // Return early if the message has no guild
            return;
        };

        // Strip the command prefix; the remainder may be empty (bare "!icalbot") or
        // start with a space followed by the subcommand.
        let Some(after_prefix) = msg.content.strip_prefix(CalendarBot::BOT_COMMAND_PREFIX) else {
            return;
        };

        let cmd = after_prefix.trim();

        // Dispatch based on the command
        match cmd {
            "refresh" => {
                let reply = match self.refresh_guild_events(ctx.clone(), guild_id).await {
                    Some(result) => format!(
                        "**Refresh complete**\n\
                         Total events: {}\n\
                         Past events: {}\n\
                         Future events: {}\n\
                         Refreshed events: {}",
                        result.total_events,
                        result.past_events,
                        result.future_events,
                        result.refreshed_events,
                    ),
                    None => "Failed to refresh events.".to_string(),
                };
                _ = msg.reply_mention(&ctx, reply).await;
            }
            "version" => _ = msg.reply_mention(&ctx, self.version.clone()).await,

            // Bare command — show the calendar URL if a discriminator is set.
            "" => {
                let gid = guild_id.get().to_string();
                match GuildRecord::lookup(&gid, &self.state.database).await {
                    Ok(Some(guild)) => match &guild.discriminator {
                        Some(disc) => {
                            let url = self.state.calendar_url(disc);
                            _ = msg.reply_mention(&ctx, format!("Calendar URL: {}", url)).await;
                        }
                        None => {
                            _ = msg
                                .reply_mention(
                                    &ctx,
                                    "No calendar configured. Use `!icalbot reset` or `!icalbot set <id>` to create one.",
                                )
                                .await;
                        }
                    },
                    Ok(None) => {
                        _ = msg.reply_mention(&ctx, "Guild not found in the database.").await;
                    }
                    Err(e) => {
                        _ = msg
                            .reply_mention(&ctx, format!("Error looking up guild: {}", e))
                            .await;
                    }
                }
            }

            // reset — generate a random discriminator
            "reset" => {
                let discriminator = GuildRecord::generate_discriminator();
                let gid = guild_id.get().to_string();
                match GuildRecord::set_discriminator(&gid, &discriminator, &self.state.database).await {
                    None => {
                        let url = self.state.calendar_url(&discriminator);
                        _ = msg
                            .reply_mention(
                                &ctx,
                                format!(
                                    "Guild discriminator set to `{}`\nCalendar URL: {}",
                                    discriminator, url
                                ),
                            )
                            .await;
                    }
                    Some(err) => {
                        _ = msg
                            .reply_mention(&ctx, format!("Failed to set discriminator: {}", err))
                            .await;
                    }
                }
            }

            // set <id> — set a user-supplied discriminator
            _ if cmd == "set" || cmd.starts_with("set ") => {
                let id = cmd.strip_prefix("set").unwrap().trim();
                if id.is_empty() {
                    _ = msg
                        .reply_mention(&ctx, "Usage: `!icalbot set <id>` — provide an ID to use.")
                        .await;
                    return;
                }
                if !GuildRecord::is_url_safe(id) {
                    _ = msg
                        .reply_mention(
                            &ctx,
                            "Invalid ID. The ID must contain only URL-safe characters \
                             (letters, digits, `-`, `.`, `_`, `~`).",
                        )
                        .await;
                    return;
                }
                let gid = guild_id.get().to_string();
                match GuildRecord::set_discriminator(&gid, id, &self.state.database).await {
                    None => {
                        let url = self.state.calendar_url(id);
                        _ = msg
                            .reply_mention(
                                &ctx,
                                format!("Guild discriminator set to `{}`\nCalendar URL: {}", id, url),
                            )
                            .await;
                    }
                    Some(err) => {
                        _ = msg
                            .reply_mention(&ctx, format!("Failed to set discriminator: {}", err))
                            .await;
                    }
                }
            }

            // delete — remove the discriminator
            "delete" => {
                let gid = guild_id.get().to_string();
                match GuildRecord::clear_discriminator(&gid, &self.state.database).await {
                    None => {
                        _ = msg.reply_mention(&ctx, "Calendar discriminator removed.").await;
                    }
                    Some(err) => {
                        _ = msg
                            .reply_mention(&ctx, format!("Failed to remove discriminator: {}", err))
                            .await;
                    }
                }
            }

            "dump_calendar" => match {
                let name = guild_id.name(&ctx.cache).unwrap_or_default();
                make_calendar(&guild_id.get().to_string(), &name, &self.state.database).await
            } {
                Ok(cal) => {
                    _ = msg
                        .reply_mention(ctx, format!("```plaintext\n{}\n```", cal))
                        .await
                }
                Err(e) => {
                    _ = msg
                        .reply_mention(
                            ctx,
                            format!(
                                "I had an error rendering your calendar...\n\n```plaintext\n{}\n```",
                                e
                            ),
                        )
                        .await
                }
            },

            "help" => {
                let help_text = "\
**iCalBot Commands**\n\
`!icalbot` — Show the calendar URL for this server\n\
`!icalbot help` — Show this help message\n\
`!icalbot reset` — Generate a new random calendar ID\n\
`!icalbot set <id>` — Set a custom calendar ID (URL-safe characters only)\n\
`!icalbot delete` — Remove the calendar ID (disables the calendar)\n\
`!icalbot refresh` — Re-sync scheduled events from Discord\n\
`!icalbot version` — Show the bot version\n\
`!icalbot dump_calendar` — Print the raw iCalendar output";
                _ = msg.reply_mention(&ctx, help_text).await;
            }

            unknown => println!("Unknown Command: {unknown}"),
        }
    }

    async fn guild_scheduled_event_create(&self, ctx: Context, event: ScheduledEvent)
    {
        print!(
            "[guild_scheduled_event_create]: {:?}/{} ",
            event.guild_id.name(&ctx.cache),
            event.name
        );
        let event_details: EventDetails = event.into();
        event_details.create(&self.state.database).await;
        println!("Success");
    }

    async fn guild_scheduled_event_update(&self, ctx: Context, event: ScheduledEvent)
    {
        print!(
            "[guild_scheduled_event_update]: {:?}/{} ",
            event.guild_id.name(&ctx.cache),
            event.name
        );
        let event_details: EventDetails = event.into();
        event_details.update(&self.state.database).await;
        println!("Success");
    }

    async fn guild_scheduled_event_delete(&self, ctx: Context, event: ScheduledEvent)
    {
        print!(
            "[guild_scheduled_event_delete]: {:?}/{} ",
            event.guild_id.name(&ctx.cache),
            event.name
        );
        let event_details: EventDetails = event.into();
        event_details.delete(&self.state.database).await;
        println!("Success");
    }
}
