
CREATE TABLE IF NOT EXISTS bot.events (
    [id]               INTEGER PRIMARY KEY,
    [discord_event_id] INTEGER UNIQUE,
    [name]             TEXT NOT NULL,
    [description]      TEXT,
    [start_time]       TIMESTAMPTZ NOT NULL,
    [duration_seconds] INTEGER NOT NULL,
    [location]         TEXT,
    [created_at]       TIMESTAMPTZ DEFAULT NOW(),
    [updated_at]       TIMESTAMPTZ DEFAULT NOW()
);
