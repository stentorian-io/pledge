//! Interlint API Server
//!
//! Provides REST API endpoints for comparing public interfaces and detecting breaking changes.

mod binary;
mod config;
mod diff;
mod handlers;
mod response;
mod routes;
mod state;

use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use crate::config::Config;
use crate::routes::create_router;
use crate::state::AppState;

#[tokio::main]
async fn main() {
    // Initialize tracing
    FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .compact()
        .init();

    info!("Starting Interlint API server");

    // Load configuration
    let config = Config::from_env();

    // Create application state
    let state = AppState::new();

    // Build router
    let app = create_router(state);

    // Get socket address
    let addr = config.socket_addr();
    info!("Listening on {}", addr);

    // Start server
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
