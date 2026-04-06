use std::error::Error;

// Define a custom error type for the bot
#[derive(Debug, Clone)]
pub struct BotError
{
    message: String,
}

impl BotError
{
    pub fn new(message: String) -> Self
    {
        BotError { message }
    }
}

impl Error for BotError {}

impl std::fmt::Display for BotError
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        write!(f, "{}", self.message)
    }
}
