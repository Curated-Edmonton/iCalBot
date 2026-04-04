CREATE TABLE IF NOT EXISTS events (
    [event_id]    TEXT    NOT NULL UNIQUE     ,
    [guild_id]    TEXT    NOT NULL            ,
    [title]       TEXT    NOT NULL            ,
    [description] TEXT                        ,
    [location]    TEXT                        ,
    [deleted]     BOOLEAN NOT NULL DEFAULT 0  ,
    [sequence]    INTEGER NOT NULL DEFAULT 0  ,
    [start_time]  TEXT    NOT NULL            ,
    [end_time]    TEXT    NOT NULL            ,
    [created_at]  TEXT    NOT NULL            ,
    [updated_at]  TEXT    NOT NULL
);
