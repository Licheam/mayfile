use crate::utils::{generate_token, now_ts};
use sqlx::{Row, SqlitePool};

pub async fn ensure_schema(pool: &SqlitePool) {
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
    let mut has_original_duration = false;
    for column in columns {
        let name: String = column.get("name");
        if name == "is_public" {
            has_is_public = true;
        }
        if name == "original_duration" {
            has_original_duration = true;
        }
    }
    if !has_is_public {
        sqlx::query("ALTER TABLE pastes ADD COLUMN is_public INTEGER NOT NULL DEFAULT 0")
            .execute(pool)
            .await
            .unwrap();
    }
    if !has_original_duration {
        sqlx::query(
            "ALTER TABLE pastes ADD COLUMN original_duration INTEGER NOT NULL DEFAULT 86400",
        )
        .execute(pool)
        .await
        .unwrap();
        // For existing rows, try to calculate duration or use default
        sqlx::query("UPDATE pastes SET original_duration = expires_at - created_at WHERE original_duration = 86400 AND expires_at IS NOT NULL AND created_at IS NOT NULL")
            .execute(pool)
            .await
            .unwrap();
    }
}

pub async fn cleanup_expired(pool: &SqlitePool) {
    sqlx::query("DELETE FROM pastes WHERE expires_at <= strftime('%s','now')")
        .execute(pool)
        .await
        .unwrap();
}

pub async fn enforce_size_limit(pool: &SqlitePool, max: i64, reserve: i64) {
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

pub async fn enforce_total_content_length(pool: &SqlitePool, max: i64, reserve: i64) {
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

pub async fn insert_paste(
    pool: &SqlitePool,
    title: String,
    content: String,
    expires_at: i64,
    original_duration: i64,
    token_length: usize,
    language: String,
    max_views: Option<i64>,
    is_public: bool,
) -> Result<String, sqlx::Error> {
    let mut token = generate_token(token_length);
    for _ in 0..5 {
        let result = sqlx::query(
            r#"
            INSERT INTO pastes (token, title, content, expires_at, original_duration, language, max_views, is_public)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&token)
        .bind(&title)
        .bind(&content)
        .bind(expires_at)
        .bind(original_duration)
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        ensure_schema(&pool).await;
        pool
    }

    #[tokio::test]
    async fn test_insert_and_retrieve_paste() {
        let pool = setup_test_db().await;
        let token = insert_paste(
            &pool,
            "Test Title".to_string(),
            "Test Content".to_string(),
            now_ts() + 3600,
            3600,
            8,
            "rust".to_string(),
            None,
            true,
        )
        .await
        .unwrap();

        let row: (String, String) =
            sqlx::query_as("SELECT title, content FROM pastes WHERE token = ?")
                .bind(&token)
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(row.0, "Test Title");
        assert_eq!(row.1, "Test Content");
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let pool = setup_test_db().await;
        // Insert expired paste
        sqlx::query("INSERT INTO pastes (token, title, content, expires_at) VALUES (?, ?, ?, ?)")
            .bind("old")
            .bind("Old")
            .bind("Old content")
            .bind(now_ts() - 3600)
            .execute(&pool)
            .await
            .unwrap();

        cleanup_expired(&pool).await;

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pastes WHERE token = 'old'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}
