use crate::models::{AppConfig, I18n, I18nConfig};
use serde::Deserialize;
use std::fs;

pub fn load_config() -> AppConfig {
    read_toml("config/app.toml")
}

pub fn load_i18n(config: &I18nConfig) -> I18n {
    I18n {
        zh: read_toml(&config.zh),
        en: read_toml(&config.en),
    }
}

pub fn read_toml<T: for<'de> Deserialize<'de>>(path: &str) -> T {
    let content = fs::read_to_string(path).expect(&format!("Failed to read {}", path));
    toml::from_str(&content).expect(&format!("Failed to parse {}", path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestConfig {
        key: String,
        value: i32,
    }

    #[test]
    fn test_read_toml() {
        let mut tmp_file = NamedTempFile::new().unwrap();
        writeln!(tmp_file, "key = 'hello'\nvalue = 42").unwrap();

        let config: TestConfig = read_toml(tmp_file.path().to_str().unwrap());
        assert_eq!(config, TestConfig { key: "hello".to_string(), value: 42 });
    }
}
