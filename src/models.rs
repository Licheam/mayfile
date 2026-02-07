use askama::Template;
use serde::Deserialize;
use sqlx::FromRow;

#[derive(Clone, FromRow)]
pub struct Paste {
    pub title: String,
    pub content: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub language: String,
    pub views: i64,
    pub max_views: Option<i64>,
    pub is_public: bool,
    pub original_duration: i64,
}

#[derive(Clone, FromRow)]
pub struct RawPaste {
    pub content: String,
    pub views: i64,
    pub max_views: Option<i64>,
}

#[derive(Clone, FromRow)]
pub struct PublicPaste {
    pub token: String,
    pub title: String,
    pub content: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub language: String,
    pub original_duration: i64,
}

#[derive(Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub paste: PasteConfig,
    pub i18n: I18nConfig,
}

#[derive(Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Deserialize)]
pub struct PasteConfig {
    pub db_path: String,
    pub default_expires_secs: i64,
    pub expires_options_secs: Vec<i64>,
    pub default_token_length: usize,
    pub token_lengths: Vec<usize>,
    pub max_content_length: usize,
    pub max_total_content_length: i64,
    pub max_pastes: i64,
}

#[derive(Clone, Deserialize)]
pub struct I18nConfig {
    pub zh: String,
    pub en: String,
}

#[allow(dead_code)]
#[derive(Clone, Deserialize)]
pub struct Strings {
    pub lang: String,
    pub app_title: String,
    pub heading: String,
    pub slogan: String,
    pub label_title: String,
    pub placeholder_title: String,
    pub label_content: String,
    pub label_expires: String,
    pub label_token_length: String,
    pub label_language: String,
    pub button_create: String,
    pub result_placeholder: String,
    pub result_title: String,
    pub result_open: String,
    pub result_expires_label: String,
    pub detail_back: String,
    pub detail_expires_label: String,
    pub detail_copy: String,
    pub detail_copy_done: String,
    pub detail_raw: String,
    pub detail_new_paste: String,
    pub not_found: String,
    pub not_found_title: String,
    pub not_found_desc: String,
    pub content_too_long: String,
    pub aria_short_link: String,
    pub duration_expired: String,
    pub duration_seconds: String,
    pub duration_minutes: String,
    pub duration_hours: String,
    pub duration_days: String,
    pub expires_seconds_one: String,
    pub expires_seconds_many: String,
    pub expires_minutes_one: String,
    pub expires_minutes_many: String,
    pub expires_hours_one: String,
    pub expires_hours_many: String,
    pub expires_days_one: String,
    pub expires_days_many: String,
    pub token_length_label: String,
    pub language_auto: String,
    pub language_plaintext: String,
    pub language_rust: String,
    pub language_python: String,
    pub language_javascript: String,
    pub language_typescript: String,
    pub language_go: String,
    pub language_java: String,
    pub language_cpp: String,
    pub language_html: String,
    pub language_css: String,
    pub language_json: String,
    pub language_yaml: String,
    pub language_sql: String,
    pub language_bash: String,
    pub label_burn: String,
    pub label_burn_views: String,
    pub detail_remaining_views: String,
    pub detail_zero_views: String,
    pub stat_total_pastes: String,
    pub stat_faded: String,
    pub label_public: String,
    pub label_public_tooltip: String,
    pub explore_title: String,
    pub explore_hint: String,
    pub explore_nav_prev: String,
    pub explore_nav_next: String,
    pub explore_empty: String,
    pub explore_swipe_hint: String,
    pub explore_count: String,
    pub explore_go: String,
    pub life_remaining: String,
    pub life_vibrant: String,
    pub life_fading: String,
    pub life_dying: String,
    pub button_renew: String,
    pub renew_success: String,
    pub button_fork: String,
}

#[derive(Clone)]
pub struct I18n {
    pub zh: Strings,
    pub en: Strings,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lang {
    Zh,
    En,
}

impl I18n {
    pub fn strings(&self, lang: Lang) -> Strings {
        match lang {
            Lang::Zh => self.zh.clone(),
            Lang::En => self.en.clone(),
        }
    }
}

// Template structs also go here as they are data models for the views
#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate {
    pub strings: Strings,
    pub expires_options: Vec<ExpiresOption>,
    pub token_length_options: Vec<TokenLengthOption>,
    pub language_options: Vec<LanguageOption>,
    pub total_pastes: String,
    pub public_count: i64,

    // Fork data
    pub fork_title: Option<String>,
    pub fork_content: Option<String>,
    pub fork_token: Option<String>,
}

#[derive(Deserialize)]
pub struct IndexQuery {
    pub lang: Option<String>,
    pub fork: Option<String>,
}

#[derive(Template)]
#[template(path = "detail.html")]
pub struct DetailTemplate {
    pub item: Paste,
    pub strings: Strings,
    pub token: String,
    pub language_label: String,
    pub remaining_views: Option<String>,
}

#[derive(Template)]
#[template(path = "item.html")]
pub struct ResultTemplate {
    pub path: String,
    pub expires_in: String,
    pub strings: Strings,
    pub language_label: String,
    pub remaining_views: Option<String>,
    pub total_pastes: String,
}

#[derive(Template)]
#[template(path = "404.html")]
pub struct NotFoundTemplate {
    pub strings: Strings,
    pub faded_count: String,
}

#[derive(Template)]
#[template(path = "explore.html")]
pub struct ExploreTemplate {
    pub strings: Strings,
    pub pastes: Vec<PublicPaste>,
    pub total: i64,
    pub now_ts: i64,
    pub max_expires_secs: i64,
}

#[derive(Clone)]
pub struct ExpiresOption {
    pub value: i64,
    pub label: String,
    pub selected: bool,
}

#[derive(Clone)]
pub struct TokenLengthOption {
    pub value: usize,
    pub label: String,
    pub selected: bool,
}

#[derive(Clone)]
pub struct LanguageOption {
    pub value: String,
    pub label: String,
    pub selected: bool,
}

#[derive(Deserialize)]
pub struct ExploreQuery {
    pub offset: Option<i64>,
}

#[derive(Deserialize)]
pub struct PasteForm {
    pub title: Option<String>,
    pub content: String,
    pub expires_in: Option<i64>,
    pub token_length: Option<usize>,
    pub language: Option<String>,
    pub max_views: Option<String>,
    pub is_public: Option<String>,
}

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::SqlitePool,
    pub config: AppConfig,
    pub i18n: I18n,
}
