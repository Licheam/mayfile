use rand::{TryRngCore, rngs::OsRng};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Generate a random alphanumeric token of the given length.
pub fn generate_token(length: usize) -> String {
    let alphabet = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut bytes = vec![0u8; length];
    let mut rng = OsRng;
    rng.try_fill_bytes(&mut bytes).unwrap();
    bytes
        .into_iter()
        .map(|value| alphabet[(value % 62) as usize] as char)
        .collect()
}

/// Get the current timestamp in seconds.
pub fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Resolve a relative path against the current working directory.
pub fn resolve_path(path: &str) -> PathBuf {
    let raw = PathBuf::from(path);
    if raw.is_absolute() {
        raw
    } else {
        std::env::current_dir().unwrap().join(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_token_length() {
        assert_eq!(generate_token(8).len(), 8);
        assert_eq!(generate_token(16).len(), 16);
    }

    #[test]
    fn test_generate_token_uniqueness() {
        let t1 = generate_token(10);
        let t2 = generate_token(10);
        assert_ne!(t1, t2);
    }

    #[test]
    fn test_now_ts() {
        let t1 = now_ts();
        assert!(t1 > 0);
    }
}
