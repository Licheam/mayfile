use crate::db;
use crate::models::*;
use crate::utils::{now_ts};
use askama::Template;
use axum::{
    extract::{Form, Path, Query, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CONTENT_DISPOSITION, CONTENT_TYPE, COOKIE, SET_COOKIE},
    },
    response::{Html, IntoResponse, Redirect},
};
use std::collections::HashMap;

pub async fn renew_paste(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> impl IntoResponse {
    let params = HashMap::new();
    let (lang, _) = select_language(&headers, &params);
    let strings = state.i18n.strings(lang);

    let item: Option<(i64, i64, i64, bool, Option<i64>)> = sqlx::query_as(
        "SELECT created_at, expires_at, original_duration, is_public, max_views FROM pastes WHERE token = ?"
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    if let Some((_, expires_at, original_duration, is_public, max_views)) = item {
        if !is_public || max_views.is_some() {
            return (StatusCode::FORBIDDEN, "Not allowed").into_response();
        }

        let now = now_ts();
        let remaining = expires_at - now;

        if remaining < (original_duration / 2) {
            let new_expires_at = now + original_duration;
            sqlx::query("UPDATE pastes SET expires_at = ? WHERE token = ?")
                .bind(new_expires_at)
                .bind(&token)
                .execute(&state.pool)
                .await
                .ok();

            let mut headers = HeaderMap::new();
            let trigger_val = format!(r#"{{"renewed": {{"token": "{}", "expires": {}}}}}"#, token, new_expires_at);
            headers.insert("HX-Trigger", HeaderValue::from_str(&trigger_val).unwrap());

            return (
                StatusCode::OK,
                headers,
                Html(format!(
                    r#"<span class="renew-success">{}</span>"#,
                    strings.renew_success
                )),
            )
                .into_response();
        }
    }

    StatusCode::BAD_REQUEST.into_response()
}

pub async fn index(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    db::cleanup_expired(&state.pool).await;
    db::enforce_size_limit(&state.pool, state.config.paste.max_pastes, 0).await;
    let (lang, set_cookie) = select_language(&headers, &params);
    let strings = state.i18n.strings(lang);
    let expires_options = build_expires_options(&state.config.paste, &strings);
    let token_length_options = build_token_length_options(&state.config.paste, &strings);
    let language_options = build_language_options(&strings);

    let max_id: i64 = sqlx::query_scalar("SELECT MAX(id) FROM pastes")
        .fetch_one(&state.pool)
        .await
        .unwrap_or(Some(0))
        .unwrap_or(0);

    let total_pastes = strings.stat_total_pastes.replace("{}", &max_id.to_string());

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

pub async fn create_paste(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<PasteForm>,
) -> impl IntoResponse {
    db::cleanup_expired(&state.pool).await;
    db::enforce_size_limit(&state.pool, state.config.paste.max_pastes, 1).await;
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
    db::enforce_total_content_length(
        &state.pool,
        state.config.paste.max_total_content_length,
        content_length as i64,
    )
    .await;
    let expires_in = normalize_expires_in(form.expires_in, &state.config.paste);
    let token_length = normalize_token_length(form.token_length, &state.config.paste);
    let language = normalize_language(form.language);
    let max_views = normalize_max_views(form.max_views.clone());
    let is_public =
        form.is_public.as_ref().map(|s| s == "on").unwrap_or(false) && max_views.is_none();
    let expires_at = now_ts() + expires_in;
    let title = normalize_title(form.title, &form.content);
    let token = match db::insert_paste(
        &state.pool,
        title,
        form.content,
        expires_at,
        expires_in,
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

    let max_id: i64 = sqlx::query_scalar("SELECT MAX(id) FROM pastes")
        .fetch_one(&state.pool)
        .await
        .unwrap_or(Some(0))
        .unwrap_or(0);
    let total_pastes = strings.stat_total_pastes.replace("{}", &max_id.to_string());

    let remaining_views = if let Some(max) = max_views {
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

pub async fn view_paste(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(token): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    db::cleanup_expired(&state.pool).await;
    db::enforce_size_limit(&state.pool, state.config.paste.max_pastes, 0).await;
    let (lang, set_cookie) = select_language(&headers, &params);
    let strings = state.i18n.strings(lang);
    let item: Option<Paste> = sqlx::query_as(
        r#"
        SELECT title, content, created_at, expires_at, language, views, max_views, is_public, original_duration
        FROM pastes
        WHERE token = ? AND expires_at > strftime('%s','now')
        "#,
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

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

pub async fn view_paste_raw(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> impl IntoResponse {
    db::cleanup_expired(&state.pool).await;
    db::enforce_size_limit(&state.pool, state.config.paste.max_pastes, 0).await;
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
    .unwrap_or(None);

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

pub async fn explore(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    db::cleanup_expired(&state.pool).await;
    let (lang, set_cookie) = select_language(&headers, &params);
    let strings = state.i18n.strings(lang);

    let pastes: Vec<PublicPaste> = sqlx::query_as(
        r#"
        SELECT token, title, content, created_at, expires_at, language, original_duration
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

    let max_expires_secs = state
        .config
        .paste
        .expires_options_secs
        .iter()
        .copied()
        .max()
        .unwrap_or(86400 * 7);

    let body = ExploreTemplate {
        strings,
        pastes,
        total,
        now_ts: now_ts(),
        max_expires_secs,
    }
    .render()
    .unwrap();

    let mut response = Html(body).into_response();
    if let Some(cookie) = set_cookie {
        response.headers_mut().insert(SET_COOKIE, cookie);
    }
    response
}

pub async fn api_explore(
    State(state): State<AppState>,
    Query(query): Query<ExploreQuery>,
) -> impl IntoResponse {
    db::cleanup_expired(&state.pool).await;
    let offset = query.offset.unwrap_or(0);

    let paste: Option<PublicPaste> = sqlx::query_as(
        r#"
        SELECT token, title, content, created_at, expires_at, language, original_duration
        FROM pastes
        WHERE is_public = 1 AND max_views IS NULL AND expires_at > strftime('%s','now')
        ORDER BY created_at DESC
        LIMIT 1 OFFSET ?
        "#,
    )
    .bind(offset)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

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
                "created_at": p.created_at,
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

// Helper functions moved from main.rs

pub fn select_language(
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

pub fn build_expires_options(config: &PasteConfig, strings: &Strings) -> Vec<ExpiresOption> {
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

pub fn build_token_length_options(config: &PasteConfig, strings: &Strings) -> Vec<TokenLengthOption> {
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

pub fn build_language_options(strings: &Strings) -> Vec<LanguageOption> {
    vec![
        LanguageOption { value: "auto".to_string(), label: strings.language_auto.clone(), selected: true },
        LanguageOption { value: "plaintext".to_string(), label: strings.language_plaintext.clone(), selected: false },
        LanguageOption { value: "rust".to_string(), label: strings.language_rust.clone(), selected: false },
        LanguageOption { value: "python".to_string(), label: strings.language_python.clone(), selected: false },
        LanguageOption { value: "javascript".to_string(), label: strings.language_javascript.clone(), selected: false },
        LanguageOption { value: "typescript".to_string(), label: strings.language_typescript.clone(), selected: false },
        LanguageOption { value: "go".to_string(), label: strings.language_go.clone(), selected: false },
        LanguageOption { value: "java".to_string(), label: strings.language_java.clone(), selected: false },
        LanguageOption { value: "cpp".to_string(), label: strings.language_cpp.clone(), selected: false },
        LanguageOption { value: "html".to_string(), label: strings.language_html.clone(), selected: false },
        LanguageOption { value: "css".to_string(), label: strings.language_css.clone(), selected: false },
        LanguageOption { value: "json".to_string(), label: strings.language_json.clone(), selected: false },
        LanguageOption { value: "yaml".to_string(), label: strings.language_yaml.clone(), selected: false },
        LanguageOption { value: "sql".to_string(), label: strings.language_sql.clone(), selected: false },
        LanguageOption { value: "bash".to_string(), label: strings.language_bash.clone(), selected: false },
    ]
}

pub fn format_expires_label(secs: i64, strings: &Strings) -> String {
    if secs >= 86400 && secs % 86400 == 0 {
        let days = secs / 86400;
        if days == 1 { strings.expires_days_one.clone() } else { strings.expires_days_many.replace("{}", &days.to_string()) }
    } else if secs >= 3600 && secs % 3600 == 0 {
        let hours = secs / 3600;
        if hours == 1 { strings.expires_hours_one.clone() } else { strings.expires_hours_many.replace("{}", &hours.to_string()) }
    } else if secs >= 60 && secs % 60 == 0 {
        let minutes = secs / 60;
        if minutes == 1 { strings.expires_minutes_one.clone() } else { strings.expires_minutes_many.replace("{}", &minutes.to_string()) }
    } else {
        if secs == 1 { strings.expires_seconds_one.clone() } else { strings.expires_seconds_many.replace("{}", &secs.to_string()) }
    }
}

pub fn normalize_expires_in(expires_in: Option<i64>, config: &PasteConfig) -> i64 {
    let value = expires_in.unwrap_or(config.default_expires_secs);
    if config.expires_options_secs.contains(&value) { value } else { config.default_expires_secs }
}

pub fn normalize_token_length(token_length: Option<usize>, config: &PasteConfig) -> usize {
    let value = token_length.unwrap_or(config.default_token_length);
    if config.token_lengths.contains(&value) { value } else { config.default_token_length }
}

pub fn normalize_language(language: Option<String>) -> String {
    let value = language.unwrap_or_else(|| "auto".to_string()).to_lowercase();
    if is_allowed_language(&value) { value } else { "auto".to_string() }
}

pub fn is_allowed_language(value: &str) -> bool {
    matches!(value, "auto" | "plaintext" | "rust" | "python" | "javascript" | "typescript" | "go" | "java" | "cpp" | "html" | "css" | "json" | "yaml" | "sql" | "bash")
}

pub fn normalize_title(title: Option<String>, content: &str) -> String {
    let trimmed = title.unwrap_or_default().trim().to_string();
    if !trimmed.is_empty() { return trimmed; }
    let first_line = content.lines().next().unwrap_or("").trim();
    if first_line.is_empty() { "Untitled".to_string() } else { first_line.chars().take(80).collect() }
}

pub fn normalize_max_views(max_views: Option<String>) -> Option<i64> {
    max_views.and_then(|s| s.parse::<i64>().ok()).filter(|&v| v > 0)
}

pub fn format_duration(expires_at: i64, strings: &Strings) -> String {
    let remaining = expires_at - now_ts();
    if remaining <= 0 { return strings.duration_expired.clone(); }
    if remaining < 60 { return strings.duration_seconds.replace("{}", &remaining.to_string()); }
    if remaining < 3600 { return strings.duration_minutes.replace("{}", &(remaining / 60).to_string()); }
    if remaining < 86400 { return strings.duration_hours.replace("{}", &(remaining / 3600).to_string()); }
    strings.duration_days.replace("{}", &(remaining / 86400).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_allowed_language() {
        assert!(is_allowed_language("rust"));
        assert!(is_allowed_language("auto"));
        assert!(!is_allowed_language("malicious"));
    }

    #[test]
    fn test_normalize_title() {
        assert_eq!(normalize_title(Some("Test".to_string()), "content"), "Test");
        assert_eq!(normalize_title(None, "First line
Second line"), "First line");
        assert_eq!(normalize_title(None, ""), "Untitled");
    }
}
