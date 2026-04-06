CREATE TABLE IF NOT EXISTS guilds (
    [guild_id]      TEXT    NOT NULL UNIQUE,
    [name]          TEXT    NOT NULL       ,
    [created_at]    TEXT    NOT NULL       ,
    [updated_at]    TEXT    NOT NULL       ,
    [discriminator] TEXT             UNIQUE
);
