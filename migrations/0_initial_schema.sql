CREATE TABLE IF NOT EXISTS events (
    [id]               INTEGER PRIMARY KEY,
    [deleted]          BOOLEAN NOT NULL DEFAULT 0,
    [discord_event_id] INTEGER UNIQUE,
    [name]             TEXT NOT NULL,
    [description]      TEXT,
    [start_time]       TEXT NOT NULL, -- stored as ISO 8601 string
    [duration_seconds] INTEGER NOT NULL,
    [location]         TEXT,
    [created_at]       TEXT DEFAULT (CURRENT_TIMESTAMP),
    [updated_at]       TEXT DEFAULT (CURRENT_TIMESTAMP)
);

CREATE TRIGGER IF NOT EXISTS events_updated_at
AFTER UPDATE ON events
FOR EACH ROW
BEGIN
  UPDATE events
  SET updated_at = CURRENT_TIMESTAMP
  WHERE id = OLD.id;
END;
