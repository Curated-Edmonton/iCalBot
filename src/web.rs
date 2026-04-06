use std::{error::Error, fmt::Display, io, sync::Arc};

use axum::{
    Router, ServiceExt,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tower::Layer;
use tower_http::normalize_path::{NormalizePath, NormalizePathLayer};

use crate::{entities::GuildRecord, icalendar::make_calendar, state::AppState};


#[derive(Debug)]
pub struct WebServerError
{
    message: String,
}

impl Error for WebServerError {}

impl Display for WebServerError
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        write!(f, "Web Error: {}", self.message)
    }
}

impl From<io::Error> for WebServerError
{
    fn from(value: io::Error) -> Self
    {
        Self {
            message: format!("IO Error: {}", value),
        }
    }
}

/// GET /health
/// Runs a SELECT query to verify database connectivity.
async fn health_check(State(state): State<Arc<AppState>>) -> (StatusCode, String)
{
    match sqlx::query("SELECT 1").execute(&state.database).await {
        Ok(_) => (StatusCode::OK, "OK".to_string()),
        Err(e) => {
            eprintln!("Healthcheck Database error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Unknown error".to_string())
        }
    }
}

/// GET / and GET /index.html
/// Responds with a simple HTML page that includes the version of the bot.
async fn hello_world() -> Html<String>
{
    let version = env!("BUILD_VERSION");
    Html(format!(
        "<!DOCTYPE html><html><head><title>iCalBot</title></head>\
         <body><h1>Hello World</h1><p>Version: {version}</p></body></html>"
    ))
}

/// GET /ical/:calendar_discriminator/ical.ics
/// Returns the calendar for the Guild associated with the given discriminator.
async fn ical_service(
    State(state): State<Arc<AppState>>,
    Path(calendar_discriminator): Path<String>,
) -> Response
{
    // Look up the guild by its discriminator
    let guild: Option<GuildRecord> = match sqlx::query_as("SELECT * FROM guilds WHERE discriminator = ?")
        .bind(&calendar_discriminator)
        .fetch_optional(&state.database)
        .await
    {
        Ok(g) => g,
        Err(e) => {
            eprintln!("Database error looking up discriminator: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    };

    let Some(guild) = guild else {
        return (StatusCode::NOT_FOUND, "Calendar not found").into_response();
    };

    // Generate the calendar
    let calendar = match make_calendar(&guild.guild_id, &guild.name, &state.database).await {
        Ok(cal) => cal,
        Err(e) => {
            eprintln!("Error generating calendar for guild {}: {}", guild.guild_id, e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to generate calendar").into_response();
        }
    };

    // Return with iCalendar content type
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/calendar; charset=utf-8"),
            (header::CONTENT_DISPOSITION, "inline; filename=\"ical.ics\""),
        ],
        calendar.to_string(),
    )
        .into_response()
}

pub async fn listen_and_serve(
    state: Arc<AppState>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), WebServerError>
{
    let listen_address = state.listen_address();
    let base_url = state.base_url.clone();

    println!(
        "Starting web server with state directory {} and database {}",
        state.state_directory.display(),
        state.database_url
    );

    let listener = TcpListener::bind(&listen_address).await?;
    let actual_addr = listener.local_addr()?;

    // Extract the path prefix from the base URL (everything after the host:port).
    // Skip past the "://" in schemes like "http://..." before looking for the path slash.
    let after_scheme = base_url.find("://").map(|i| i + 3).unwrap_or(0);
    let base_path = base_url[after_scheme..]
        .find('/')
        .map(|i| base_url[after_scheme + i..].trim_end_matches('/').to_string())
        .unwrap_or_default();

    // Build route tree.
    let routes = Router::new()
        .route("/health", get(health_check))
        .route("/", get(hello_world))
        .route("/index.html", get(hello_world))
        .route("/ical/{calendar_discriminator}/ical.ics", get(ical_service))
        .with_state(state);

    // Nest under the base path if one is configured.
    let app = if base_path.is_empty() {
        routes
    } else {
        Router::new().nest(&base_path, routes)
    };

    // Normalize trailing slashes so e.g. /test/ is treated the same as /test.
    let app = NormalizePathLayer::trim_trailing_slash().layer(app);
    let app =
        <NormalizePath<Router> as ServiceExt<axum::http::Request<axum::body::Body>>>::into_make_service(app);

    // Print both the local socket address and the base URL.
    println!("Web server listening on {actual_addr}, base URL: {base_url}");

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            loop {
                if shutdown.changed().await.is_ok() && *shutdown.borrow() {
                    println!("Shutdown requested. Stopping web server.");
                    break;
                }
            }
        })
        .await
        .map_err(|e| WebServerError {
            message: format!("Server error: {}", e),
        })
}
