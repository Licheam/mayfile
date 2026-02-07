mod config;
mod db;
mod handlers;
mod models;
mod utils;

use axum::{
    Router,
    routing::{get, post},
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::fs;
use tower_http::services::ServeDir;

use crate::models::AppState;
use crate::utils::resolve_path;

#[tokio::main]
async fn main() {
    let config = config::load_config();
    let i18n = config::load_i18n(&config.i18n);
    let db_path = resolve_path(&config.paste.db_path);
    
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    
    let connect_options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true);
        
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options)
        .await
        .expect("Failed to connect to database");

    db::ensure_schema(&pool).await;

    let addr = format!("{}:{}", config.server.host, config.server.port);
    let app = Router::new()
        .route("/", get(handlers::index))
        .route("/paste", post(handlers::create_paste))
        .route("/p/{token}", get(handlers::view_paste))
        .route("/p/{token}/renew", post(handlers::renew_paste))
        .route("/r/{token}", get(handlers::view_paste_raw))
        .route("/explore", get(handlers::explore))
        .route("/api/explore", get(handlers::api_explore))
        .nest_service("/assets", ServeDir::new("assets"))
        .with_state(AppState { pool, config, i18n });

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("Listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}