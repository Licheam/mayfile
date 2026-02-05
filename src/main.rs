use askama::Template;
use axum::{
    Router,
    extract::{Form, Path, Query, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CONTENT_DISPOSITION, CONTENT_TYPE, COOKIE, SET_COOKIE},
    },
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use rand::{TryRngCore, rngs::OsRng};
use serde::Deserialize;
use sqlx::Row;
use sqlx::{
    FromRow, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, path::PathBuf};
use tower_http::services::ServeDir;

#[derive(Clone, FromRow)]
struct Paste {
    title: String,
    content: String,
    expires_at: i64,
    language: String,
    views: i64,
    max_views: Option<i64>,
    is_public: bool,
}

#[derive(Clone, FromRow)]
struct RawPaste {
    content: String,
    views: i64,
    max_views: Option<i64>,
}

#[derive(Clone, FromRow)]
struct PublicPaste {
    token: String,
    title: String,
    content: String,
    expires_at: i64,
    language: String,
}

#[derive(Clone)]
struct AppState {
    pool: SqlitePool,
    config: AppConfig,
    i18n: I18n,
}

#[derive(Clone, Deserialize)]
struct AppConfig {
    server: ServerConfig,
    paste: PasteConfig,
    i18n: I18nConfig,
}

#[derive(Clone, Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,
}

#[derive(Clone, Deserialize)]
struct PasteConfig {
    db_path: String,
    default_expires_secs: i64,
    expires_options_secs: Vec<i64>,
    default_token_length: usize,
    token_lengths: Vec<usize>,
    max_content_length: usize,
    max_total_content_length: i64,
    max_pastes: i64,
}

#[derive(Clone, Deserialize)]
struct I18nConfig {
    zh: String,
    en: String,
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    strings: Strings,
    expires_options: Vec<ExpiresOption>,
    token_length_options: Vec<TokenLengthOption>,
    language_options: Vec<LanguageOption>,
    total_pastes: String,
    public_count: i64,
}

#[derive(Template)]
#[template(path = "detail.html")]
struct DetailTemplate {
    item: Paste,
    expires_in: String,
    strings: Strings,
    token: String,
    language_label: String,
    remaining_views: Option<String>,
}

#[derive(Template)]
#[template(path = "item.html")]
struct ResultTemplate {
    path: String,
    expires_in: String,
    strings: Strings,
    language_label: String,
    remaining_views: Option<String>,
    total_pastes: String,
}

#[derive(Template)]
#[template(path = "404.html")]
struct NotFoundTemplate {
    strings: Strings,
    faded_count: String,
}

#[derive(Template)]
#[template(path = "explore.html")]
struct ExploreTemplate {
    strings: Strings,
    pastes: Vec<PublicPaste>,
    total: i64,
}

#[derive(Deserialize)]
struct ExploreQuery {
    offset: Option<i64>,
}

#[derive(Deserialize)]
struct PasteForm {
    title: Option<String>,
    content: String,
    expires_in: Option<i64>,
    token_length: Option<usize>,
    language: Option<String>,
    max_views: Option<String>,
    is_public: Option<String>,
}

#[tokio::main]
async fn main() {
    let config = load_config();
    let i18n = load_i18n(&config.i18n);
    let db_path = resolve_path(&config.paste.db_path);
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let connect_options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true);
    let pool: SqlitePool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options)
        .await
        .unwrap();

    ensure_schema(&pool).await;

    let addr = format!("{}:{}", config.server.host, config.server.port);
    let app = Router::new()
        .route("/", get(index))
        .route("/paste", post(create_paste))
        .route("/p/{token}", get(view_paste))
        .route("/r/{token}", get(view_paste_raw))
        .route("/explore", get(explore))
        .route("/api/explore", get(api_explore))
        .nest_service("/assets", ServeDir::new("assets"))
        .with_state(AppState { pool, config, i18n });

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn index(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    cleanup_expired(&state.pool).await;
    enforce_size_limit(&state.pool, state.config.paste.max_pastes, 0).await;
    let (lang, set_cookie) = select_language(&headers, &params);
    let strings = state.i18n.strings(lang);
    let expires_options = build_expires_options(&state.config.paste, &strings);
    let token_length_options = build_token_length_options(&state.config.paste, &strings);
    let language_options = build_language_options(&strings);

    // Get total pastes count (including expired/deleted ones? No, usually existing.
    // But the prompt says "12,345 moments have been born here".
    // Since we delete expired pastes, we can't know the historical total unless we track it separately or use AUTOINCREMENT id.
    // The ID is AUTOINCREMENT, so the max ID is roughly the total born moments.
    // Let's query MAX(id).
    let max_id: i64 = sqlx::query_scalar("SELECT MAX(id) FROM pastes")
        .fetch_one(&state.pool)
        .await
        .unwrap_or(Some(0))
        .unwrap_or(0);

    let total_pastes = strings.stat_total_pastes.replace("{}", &max_id.to_string());

    // Get public pastes count for explore entry
    let public_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pastes WHERE is_public = 1 AND expires_at > strftime('%s','now')",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    let body = IndexTemplate {
        strings,
        expires_options,
        token_length_options,
        language_options,
        total_pastes,
        public_count,
    }
    .render()
    .unwrap();

    let mut response = Html(body).into_response();
    if let Some(cookie) = set_cookie {
        response.headers_mut().insert(SET_COOKIE, cookie);
    }
    response
}

async fn create_paste(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<PasteForm>,
) -> impl IntoResponse {
    cleanup_expired(&state.pool).await;
    enforce_size_limit(&state.pool, state.config.paste.max_pastes, 1).await;
    let params = HashMap::new();
    let (lang, _) = select_language(&headers, &params);
    let strings = state.i18n.strings(lang);
    let content_length = form.content.chars().count();
    if content_length > state.config.paste.max_content_length {
        let message = strings
            .content_too_long
            .replace("{}", &state.config.paste.max_content_length.to_string());
        return (StatusCode::BAD_REQUEST, Html(message)).into_response();
    }
    if (content_length as i64) > state.config.paste.max_total_content_length {
        let message = strings.content_too_long.replace(
            "{}",
            &state.config.paste.max_total_content_length.to_string(),
        );
        return (StatusCode::BAD_REQUEST, Html(message)).into_response();
    }
    enforce_total_content_length(
        &state.pool,
        state.config.paste.max_total_content_length,
        content_length as i64,
    )
    .await;
    let expires_in = normalize_expires_in(form.expires_in, &state.config.paste);
    let token_length = normalize_token_length(form.token_length, &state.config.paste);
    let language = normalize_language(form.language);
    let max_views = normalize_max_views(form.max_views.clone());
    // is_public is only allowed if max_views is not set (no burn-on-read)
    let is_public =
        form.is_public.as_ref().map(|s| s == "on").unwrap_or(false) && max_views.is_none();
    let expires_at = now_ts() + expires_in;
    let title = normalize_title(form.title, &form.content);
    let token = match insert_paste(
        &state.pool,
        title,
        form.content,
        expires_at,
        token_length,
        language.clone(),
        max_views,
        is_public,
    )
    .await
    {
        Ok(result) => result,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html("Failed".to_string()),
            )
                .into_response();
        }
    };
    let expires_in_text = format_duration(expires_at, &strings);

    let language_label = build_language_options(&strings)
        .into_iter()
        .find(|opt| opt.value == language)
        .map(|opt| opt.label)
        .unwrap_or_else(|| language.clone());

    // Get updated total pastes count for OOB swap
    let max_id: i64 = sqlx::query_scalar("SELECT MAX(id) FROM pastes")
        .fetch_one(&state.pool)
        .await
        .unwrap_or(Some(0))
        .unwrap_or(0);
    let total_pastes = strings.stat_total_pastes.replace("{}", &max_id.to_string());

    let remaining_views = if let Some(max) = max_views {
        // Newly created paste has 0 views, but we show remaining views
        // Logic: if max=5, views=0, remaining=5.
        // Wait, in view_paste: (max - views - 1).max(0)
        // Here, views is effectively 0. So remaining depends on how we want to show it.
        // If the user sets "max 5 views", it has 5 views remaining.
        // The view_paste logic decreases by 1 because that view counts as 1.
        // On creation, no views yet. So simple "max" is correct?
        // Let's stick to consistent logic. If I view it, it becomes max-1.
        // Here we just display it.
        // Let's use max_views directly.
        Some(
            strings
                .detail_remaining_views
                .replace("{}", &max.to_string()),
        )
    } else {
        None
    };

    if headers.contains_key("hx-request") {
        let body = ResultTemplate {
            path: format!("/p/{}", token),
            expires_in: expires_in_text,
            strings,
            language_label,
            remaining_views,
            total_pastes,
        }
        .render()
        .unwrap();
        Html(body).into_response()
    } else {
        Redirect::to(&format!("/p/{}", token)).into_response()
    }
}

async fn view_paste(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(token): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    cleanup_expired(&state.pool).await;
    enforce_size_limit(&state.pool, state.config.paste.max_pastes, 0).await;
    let (lang, set_cookie) = select_language(&headers, &params);
    let strings = state.i18n.strings(lang);
    let item: Option<Paste> = sqlx::query_as(
        r#"
        SELECT title, content, expires_at, language, views, max_views, is_public
        FROM pastes
        WHERE token = ? AND expires_at > strftime('%s','now')
        "#,
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .unwrap();

    if let Some(ref p) = item {
        let new_views = p.views + 1;
        sqlx::query("UPDATE pastes SET views = ? WHERE token = ?")
            .bind(new_views)
            .bind(&token)
            .execute(&state.pool)
            .await
            .ok();
        if let Some(max) = p.max_views {
            if max > 0 && new_views >= max {
                sqlx::query("DELETE FROM pastes WHERE token = ?")
                    .bind(&token)
                    .execute(&state.pool)
                    .await
                    .ok();
            }
        }
    }

    let response_body = match item {
        Some(item) => {
            let language_label = build_language_options(&strings)
                .into_iter()
                .find(|opt| opt.value == item.language)
                .map(|opt| opt.label)
                .unwrap_or_else(|| item.language.clone());

            let remaining_views = if let Some(max) = item.max_views {
                let remaining = (max - item.views - 1).max(0);
                if remaining == 0 {
                    Some(strings.detail_zero_views.clone())
                } else {
                    Some(
                        strings
                            .detail_remaining_views
                            .replace("{}", &remaining.to_string()),
                    )
                }
            } else {
                None
            };

            let body = DetailTemplate {
                expires_in: format_duration(item.expires_at, &strings),
                item,
                strings,
                token,
                language_label,
                remaining_views,
            }
            .render()
            .unwrap();
            Html(body).into_response()
        }
        None => {
            let max_id: i64 = sqlx::query_scalar("SELECT MAX(id) FROM pastes")
                .fetch_one(&state.pool)
                .await
                .unwrap_or(Some(0))
                .unwrap_or(0);
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pastes")
                .fetch_one(&state.pool)
                .await
                .unwrap_or(0);
            let faded = (max_id - count).max(0);
            let faded_count = strings.stat_faded.replace("{}", &faded.to_string());

            let body = NotFoundTemplate {
                strings,
                faded_count,
            }
            .render()
            .unwrap();
            (StatusCode::NOT_FOUND, Html(body)).into_response()
        }
    };

    let mut response = response_body;
    if let Some(cookie) = set_cookie {
        response.headers_mut().insert(SET_COOKIE, cookie);
    }
    response
}

async fn view_paste_raw(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> impl IntoResponse {
    cleanup_expired(&state.pool).await;
    enforce_size_limit(&state.pool, state.config.paste.max_pastes, 0).await;
    let item: Option<RawPaste> = sqlx::query_as(
        r#"
        SELECT content, views, max_views
        FROM pastes
        WHERE token = ? AND expires_at > strftime('%s','now')
        "#,
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .unwrap();

    if let Some(ref p) = item {
        let new_views = p.views + 1;
        sqlx::query("UPDATE pastes SET views = ? WHERE token = ?")
            .bind(new_views)
            .bind(&token)
            .execute(&state.pool)
            .await
            .ok();
        if let Some(max) = p.max_views {
            if max > 0 && new_views >= max {
                sqlx::query("DELETE FROM pastes WHERE token = ?")
                    .bind(&token)
                    .execute(&state.pool)
                    .await
                    .ok();
            }
        }
    }

    match item {
        Some(item) => {
            let mut headers = HeaderMap::new();
            headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            );
            let filename = format!("paste-{}.txt", token);
            let disposition = format!("inline; filename=\"{}\"", filename);
            headers.insert(
                CONTENT_DISPOSITION,
                HeaderValue::from_str(&disposition).unwrap(),
            );
            (headers, item.content).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn explore(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    cleanup_expired(&state.pool).await;
    let (lang, set_cookie) = select_language(&headers, &params);
    let strings = state.i18n.strings(lang);

    // Get all public pastes (not burn-on-read)
    let pastes: Vec<PublicPaste> = sqlx::query_as(
        r#"
        SELECT token, title, content, expires_at, language
        FROM pastes
        WHERE is_public = 1 AND max_views IS NULL AND expires_at > strftime('%s','now')
        ORDER BY created_at DESC
        LIMIT 100
        "#,
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let total = pastes.len() as i64;

    let body = ExploreTemplate {
        strings,
        pastes,
        total,
    }
    .render()
    .unwrap();

    let mut response = Html(body).into_response();
    if let Some(cookie) = set_cookie {
        response.headers_mut().insert(SET_COOKIE, cookie);
    }
    response
}

async fn api_explore(
    State(state): State<AppState>,
    Query(query): Query<ExploreQuery>,
) -> impl IntoResponse {
    cleanup_expired(&state.pool).await;
    let offset = query.offset.unwrap_or(0);

    // Get public paste at the specified offset
    let paste: Option<PublicPaste> = sqlx::query_as(
        r#"
        SELECT token, title, content, expires_at, language
        FROM pastes
        WHERE is_public = 1 AND max_views IS NULL AND expires_at > strftime('%s','now')
        ORDER BY created_at DESC
        LIMIT 1 OFFSET ?
        "#,
    )
    .bind(offset)
    .fetch_optional(&state.pool)
    .await
    .unwrap();

    // Get total count
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pastes WHERE is_public = 1 AND max_views IS NULL AND expires_at > strftime('%s','now')"
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    match paste {
        Some(p) => {
            let json = serde_json::json!({
                "token": p.token,
                "title": p.title,
                "content": p.content,
                "expires_at": p.expires_at,
                "language": p.language,
                "index": offset,
                "total": total
            });
            axum::Json(json).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn ensure_schema(pool: &SqlitePool) {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS pastes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            token TEXT,
            title TEXT NOT NULL,
            content TEXT NOT NULL,
            language TEXT,
            created_at INTEGER NOT NULL DEFAULT (strftime('%s','now')),
            expires_at INTEGER,
            views INTEGER NOT NULL DEFAULT 0,
            max_views INTEGER
        )
        "#,
    )
    .execute(pool)
    .await
    .unwrap();

    let columns = sqlx::query("PRAGMA table_info(pastes)")
        .fetch_all(pool)
        .await
        .unwrap();
    let mut has_token = false;
    let mut has_expires_at = false;
    let mut has_language = false;
    for column in columns {
        let name: String = column.get("name");
        if name == "token" {
            has_token = true;
        }
        if name == "expires_at" {
            has_expires_at = true;
        }
        if name == "language" {
            has_language = true;
        }
    }
    if !has_token {
        sqlx::query("ALTER TABLE pastes ADD COLUMN token TEXT")
            .execute(pool)
            .await
            .unwrap();
    }
    if !has_expires_at {
        sqlx::query("ALTER TABLE pastes ADD COLUMN expires_at INTEGER")
            .execute(pool)
            .await
            .unwrap();
    }
    if !has_language {
        sqlx::query("ALTER TABLE pastes ADD COLUMN language TEXT")
            .execute(pool)
            .await
            .unwrap();
    }
    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_pastes_token ON pastes(token)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE pastes SET expires_at = strftime('%s','now') WHERE expires_at IS NULL")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE pastes SET language = 'auto' WHERE language IS NULL")
        .execute(pool)
        .await
        .unwrap();

    let columns = sqlx::query("PRAGMA table_info(pastes)")
        .fetch_all(pool)
        .await
        .unwrap();
    let mut has_views = false;
    let mut has_max_views = false;
    for column in columns {
        let name: String = column.get("name");
        if name == "views" {
            has_views = true;
        }
        if name == "max_views" {
            has_max_views = true;
        }
    }
    if !has_views {
        sqlx::query("ALTER TABLE pastes ADD COLUMN views INTEGER NOT NULL DEFAULT 0")
            .execute(pool)
            .await
            .unwrap();
    }
    if !has_max_views {
        sqlx::query("ALTER TABLE pastes ADD COLUMN max_views INTEGER")
            .execute(pool)
            .await
            .unwrap();
    }

    // Check for is_public column
    let columns = sqlx::query("PRAGMA table_info(pastes)")
        .fetch_all(pool)
        .await
        .unwrap();
    let mut has_is_public = false;
    for column in columns {
        let name: String = column.get("name");
        if name == "is_public" {
            has_is_public = true;
        }
    }
    if !has_is_public {
        sqlx::query("ALTER TABLE pastes ADD COLUMN is_public INTEGER NOT NULL DEFAULT 0")
            .execute(pool)
            .await
            .unwrap();
    }
}

async fn cleanup_expired(pool: &SqlitePool) {
    sqlx::query("DELETE FROM pastes WHERE expires_at <= strftime('%s','now')")
        .execute(pool)
        .await
        .unwrap();
}

async fn enforce_size_limit(pool: &SqlitePool, max: i64, reserve: i64) {
    let allowed = (max - reserve).max(0);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pastes")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    if count > allowed {
        let overflow = count - allowed;
        sqlx::query(
            r#"
            DELETE FROM pastes
            WHERE id IN (
                SELECT id FROM pastes
                ORDER BY expires_at ASC, id ASC
                LIMIT ?
            )
            "#,
        )
        .bind(overflow)
        .execute(pool)
        .await
        .unwrap();
    }
}

async fn enforce_total_content_length(pool: &SqlitePool, max: i64, reserve: i64) {
    let allowed = (max - reserve).max(0);
    let mut total: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(LENGTH(content)), 0) FROM pastes")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    if total <= allowed {
        return;
    }
    let rows = sqlx::query(
        r#"
        SELECT id, LENGTH(content) AS len
        FROM pastes
        ORDER BY expires_at ASC, id ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .unwrap();
    for row in rows {
        if total <= allowed {
            break;
        }
        let id: i64 = row.get("id");
        let len: i64 = row.get("len");
        sqlx::query("DELETE FROM pastes WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
        total -= len;
    }
}

async fn insert_paste(
    pool: &SqlitePool,
    title: String,
    content: String,
    expires_at: i64,
    token_length: usize,
    language: String,
    max_views: Option<i64>,
    is_public: bool,
) -> Result<String, sqlx::Error> {
    let mut token = generate_token(token_length);
    for _ in 0..5 {
        let result = sqlx::query(
            r#"
            INSERT INTO pastes (token, title, content, expires_at, language, max_views, is_public)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&token)
        .bind(&title)
        .bind(&content)
        .bind(expires_at)
        .bind(&language)
        .bind(max_views)
        .bind(is_public)
        .execute(pool)
        .await;

        match result {
            Ok(_) => return Ok(token),
            Err(err) => {
                if err
                    .as_database_error()
                    .map(|db_err| db_err.is_unique_violation())
                    .unwrap_or(false)
                {
                    token = generate_token(token_length);
                    continue;
                }
                return Err(err);
            }
        }
    }
    Err(sqlx::Error::Protocol("token collision".into()))
}

fn generate_token(length: usize) -> String {
    let alphabet = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut bytes = vec![0u8; length];
    let mut rng = OsRng;
    rng.try_fill_bytes(&mut bytes).unwrap();
    bytes
        .into_iter()
        .map(|value| alphabet[(value % 62) as usize] as char)
        .collect()
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn normalize_expires_in(expires_in: Option<i64>, config: &PasteConfig) -> i64 {
    let value = expires_in.unwrap_or(config.default_expires_secs);
    if config.expires_options_secs.contains(&value) {
        value
    } else {
        config.default_expires_secs
    }
}

fn normalize_token_length(token_length: Option<usize>, config: &PasteConfig) -> usize {
    let value = token_length.unwrap_or(config.default_token_length);
    if config.token_lengths.contains(&value) {
        value
    } else {
        config.default_token_length
    }
}

fn normalize_language(language: Option<String>) -> String {
    let value = language
        .unwrap_or_else(|| "auto".to_string())
        .to_lowercase();
    if is_allowed_language(&value) {
        value
    } else {
        "auto".to_string()
    }
}

fn format_duration(expires_at: i64, strings: &Strings) -> String {
    let remaining = expires_at - now_ts();
    if remaining <= 0 {
        return strings.duration_expired.clone();
    }
    if remaining < 60 {
        return strings
            .duration_seconds
            .replace("{}", &remaining.to_string());
    }
    if remaining < 3600 {
        return strings
            .duration_minutes
            .replace("{}", &(remaining / 60).to_string());
    }
    if remaining < 86400 {
        return strings
            .duration_hours
            .replace("{}", &(remaining / 3600).to_string());
    }
    strings
        .duration_days
        .replace("{}", &(remaining / 86400).to_string())
}

fn normalize_title(title: Option<String>, content: &str) -> String {
    let trimmed = title.unwrap_or_default().trim().to_string();
    if !trimmed.is_empty() {
        return trimmed;
    }
    let first_line = content.lines().next().unwrap_or("").trim();
    if first_line.is_empty() {
        "Untitled".to_string()
    } else {
        first_line.chars().take(80).collect()
    }
}

fn normalize_max_views(max_views: Option<String>) -> Option<i64> {
    max_views
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|&v| v > 0)
}

#[derive(Clone, Deserialize)]
struct Strings {
    lang: String,
    app_title: String,
    heading: String,
    slogan: String,
    label_title: String,
    placeholder_title: String,
    label_content: String,
    label_expires: String,
    label_token_length: String,
    label_language: String,
    button_create: String,
    result_placeholder: String,
    result_title: String,
    result_open: String,
    result_expires_label: String,
    detail_back: String,
    detail_expires_label: String,
    detail_copy: String,
    detail_copy_done: String,
    detail_raw: String,
    detail_new_paste: String,
    not_found: String,
    not_found_title: String,
    not_found_desc: String,
    content_too_long: String,
    aria_short_link: String,
    duration_expired: String,
    duration_seconds: String,
    duration_minutes: String,
    duration_hours: String,
    duration_days: String,
    expires_seconds_one: String,
    expires_seconds_many: String,
    expires_minutes_one: String,
    expires_minutes_many: String,
    expires_hours_one: String,
    expires_hours_many: String,
    expires_days_one: String,
    expires_days_many: String,
    token_length_label: String,
    language_auto: String,
    language_plaintext: String,
    language_rust: String,
    language_python: String,
    language_javascript: String,
    language_typescript: String,
    language_go: String,
    language_java: String,
    language_cpp: String,
    language_html: String,
    language_css: String,
    language_json: String,
    language_yaml: String,
    language_sql: String,
    language_bash: String,
    label_burn: String,
    label_burn_views: String,
    detail_remaining_views: String,
    detail_zero_views: String,
    stat_total_pastes: String,
    stat_faded: String,
    // Public / Explore
    label_public: String,
    label_public_tooltip: String,
    explore_title: String,
    explore_hint: String,
    explore_nav_prev: String,
    explore_nav_next: String,
    explore_empty: String,
    explore_swipe_hint: String,
    explore_count: String,
    explore_go: String,
}

#[derive(Clone)]
struct I18n {
    zh: Strings,
    en: Strings,
}

impl I18n {
    fn strings(&self, lang: Lang) -> Strings {
        match lang {
            Lang::Zh => self.zh.clone(),
            Lang::En => self.en.clone(),
        }
    }
}

#[derive(Clone, Copy)]
enum Lang {
    Zh,
    En,
}

#[derive(Clone)]
struct ExpiresOption {
    value: i64,
    label: String,
    selected: bool,
}

#[derive(Clone)]
struct TokenLengthOption {
    value: usize,
    label: String,
    selected: bool,
}

#[derive(Clone)]
struct LanguageOption {
    value: String,
    label: String,
    selected: bool,
}

fn select_language(
    headers: &HeaderMap,
    params: &HashMap<String, String>,
) -> (Lang, Option<HeaderValue>) {
    if let Some(lang) = params.get("lang") {
        match lang.as_str() {
            "zh" => {
                return (
                    Lang::Zh,
                    Some(HeaderValue::from_static(
                        "lang=zh; Path=/; Max-Age=31536000",
                    )),
                );
            }
            "en" => {
                return (
                    Lang::En,
                    Some(HeaderValue::from_static(
                        "lang=en; Path=/; Max-Age=31536000",
                    )),
                );
            }
            _ => {}
        }
    }

    if let Some(cookie) = headers.get(COOKIE).and_then(|v| v.to_str().ok()) {
        for part in cookie.split(';') {
            let part = part.trim();
            if part.starts_with("lang=") {
                match part.strip_prefix("lang=") {
                    Some("zh") => return (Lang::Zh, None),
                    Some("en") => return (Lang::En, None),
                    _ => {}
                }
            }
        }
    }

    let is_zh = headers
        .get("accept-language")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_lowercase().contains("zh"))
        .unwrap_or(true);
    if is_zh {
        (Lang::Zh, None)
    } else {
        (Lang::En, None)
    }
}

fn build_expires_options(config: &PasteConfig, strings: &Strings) -> Vec<ExpiresOption> {
    config
        .expires_options_secs
        .iter()
        .map(|value| ExpiresOption {
            value: *value,
            label: format_expires_label(*value, strings),
            selected: *value == config.default_expires_secs,
        })
        .collect()
}

fn build_token_length_options(config: &PasteConfig, strings: &Strings) -> Vec<TokenLengthOption> {
    config
        .token_lengths
        .iter()
        .map(|value| TokenLengthOption {
            value: *value,
            label: strings.token_length_label.replace("{}", &value.to_string()),
            selected: *value == config.default_token_length,
        })
        .collect()
}

fn build_language_options(strings: &Strings) -> Vec<LanguageOption> {
    vec![
        LanguageOption {
            value: "auto".to_string(),
            label: strings.language_auto.clone(),
            selected: true,
        },
        LanguageOption {
            value: "plaintext".to_string(),
            label: strings.language_plaintext.clone(),
            selected: false,
        },
        LanguageOption {
            value: "rust".to_string(),
            label: strings.language_rust.clone(),
            selected: false,
        },
        LanguageOption {
            value: "python".to_string(),
            label: strings.language_python.clone(),
            selected: false,
        },
        LanguageOption {
            value: "javascript".to_string(),
            label: strings.language_javascript.clone(),
            selected: false,
        },
        LanguageOption {
            value: "typescript".to_string(),
            label: strings.language_typescript.clone(),
            selected: false,
        },
        LanguageOption {
            value: "go".to_string(),
            label: strings.language_go.clone(),
            selected: false,
        },
        LanguageOption {
            value: "java".to_string(),
            label: strings.language_java.clone(),
            selected: false,
        },
        LanguageOption {
            value: "cpp".to_string(),
            label: strings.language_cpp.clone(),
            selected: false,
        },
        LanguageOption {
            value: "html".to_string(),
            label: strings.language_html.clone(),
            selected: false,
        },
        LanguageOption {
            value: "css".to_string(),
            label: strings.language_css.clone(),
            selected: false,
        },
        LanguageOption {
            value: "json".to_string(),
            label: strings.language_json.clone(),
            selected: false,
        },
        LanguageOption {
            value: "yaml".to_string(),
            label: strings.language_yaml.clone(),
            selected: false,
        },
        LanguageOption {
            value: "sql".to_string(),
            label: strings.language_sql.clone(),
            selected: false,
        },
        LanguageOption {
            value: "bash".to_string(),
            label: strings.language_bash.clone(),
            selected: false,
        },
    ]
}

fn is_allowed_language(value: &str) -> bool {
    matches!(
        value,
        "auto"
            | "plaintext"
            | "rust"
            | "python"
            | "javascript"
            | "typescript"
            | "go"
            | "java"
            | "cpp"
            | "html"
            | "css"
            | "json"
            | "yaml"
            | "sql"
            | "bash"
    )
}

fn format_expires_label(secs: i64, strings: &Strings) -> String {
    if secs >= 86400 && secs % 86400 == 0 {
        let days = secs / 86400;
        if days == 1 {
            strings.expires_days_one.clone()
        } else {
            strings.expires_days_many.replace("{}", &days.to_string())
        }
    } else if secs >= 3600 && secs % 3600 == 0 {
        let hours = secs / 3600;
        if hours == 1 {
            strings.expires_hours_one.clone()
        } else {
            strings.expires_hours_many.replace("{}", &hours.to_string())
        }
    } else if secs >= 60 && secs % 60 == 0 {
        let minutes = secs / 60;
        if minutes == 1 {
            strings.expires_minutes_one.clone()
        } else {
            strings
                .expires_minutes_many
                .replace("{}", &minutes.to_string())
        }
    } else {
        if secs == 1 {
            strings.expires_seconds_one.clone()
        } else {
            strings
                .expires_seconds_many
                .replace("{}", &secs.to_string())
        }
    }
}

fn load_config() -> AppConfig {
    read_toml("config/app.toml")
}

fn load_i18n(config: &I18nConfig) -> I18n {
    I18n {
        zh: read_toml(&config.zh),
        en: read_toml(&config.en),
    }
}

fn read_toml<T: for<'de> Deserialize<'de>>(path: &str) -> T {
    let content = fs::read_to_string(path).unwrap();
    toml::from_str(&content).unwrap()
}

fn resolve_path(path: &str) -> PathBuf {
    let raw = PathBuf::from(path);
    if raw.is_absolute() {
        raw
    } else {
        std::env::current_dir().unwrap().join(raw)
    }
}
