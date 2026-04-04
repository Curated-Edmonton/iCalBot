use chrono::Duration;
use icalendar::{self, Calendar, Component, Event, EventLike};
use serenity::all::{Context, GuildId};

use crate::{entities::EventDetails, errors::BotError};

pub async fn make_calendar(guild: GuildId, ctx: &Context, db: &sqlx::SqlitePool) -> Result<Calendar, BotError> {
    let guild_id = guild.get().to_string();
    let Some(guild_name) = guild.name(&ctx.cache) else {
        return Err(BotError::new("Cannot make a calendar for a nameless guild.".into()));
    };

    let mut result = Calendar::new();

    // Set the name of the calendar
    result.name(guild_name.as_str());

    // Calendar can be cached for one day
    result.ttl(&Duration::days(1));

    // TODO: Don't hardcode this
    result.timezone("America/Edmonton");

    // Fetch all the events for the guild
    let events: Vec<EventDetails> = match sqlx::query_as(
        "SELECT * FROM events WHERE guild_id = ?",
    ).bind(guild_id).fetch_all(db).await {
        Ok(events) => events,
        Err(e) => return Err(BotError::new(format!("Could not retrieve events for guild {} due to : {}.", guild, e))),
    };

    // Add each event to the iCalendar
    for e in events.iter() {
        let mut event = Event::new();
        event.status(icalendar::EventStatus::Confirmed);

        event.summary(&e.title);
        event.starts(e.start_time);
        event.ends(e.end_time);
        event.created(e.created_at);
        event.last_modified(e.updated_at);

        e.description.clone().map(|desc| event.description(desc.as_str()));
        e.location.clone().map(|loc| event.location(loc.as_str()));

        result.push(event);
    }

    // Return the final calendar
    Ok(result.done())
}


/*

EVENT Fields for Google Calendar:

    Hardcode Fields

        TRANSP:OPAQUE
        STATUS:CONFIRMED

    Set fields per 'lookup' of the calendar:

        DTSTAMP - Retrieval Time

    Set fields from db event:

        SUMMARY       - Title of the event
        DESCRIPTION   - Basic HTML of description
        DTSTART       - Start TimestampZ
        DTEND         - End TimestampZ
        CREATED       - Created TimestampZ
        LAST-MODIFIED - Updated TimestampZ

        LOCATION      - Optional: Basic one-line HTML of location
        UID           - Optional: Email address for the event organizer?
        SEQUENCE      - Default:0, If this is a recurring event, which recurrence is it?

*/
