# Mayfile

[简体中文](./README.zh-CN.md)

Born in a blink. Gone with a link.

A minimalist, high-performance, self-hosted pastebin service built with Rust.

## Features

- 🚀 **High Performance**: Built with Rust and Axum for extreme speed and low resource usage.
- 💾 **Simple Storage**: Uses SQLite for easy deployment and management (no separate database server needed).
- 🌐 **Internationalization**: Automatic language detection (English/Chinese) based on browser headers.
- ⏳ **Expiration Control**: Configurable paste expiration times.
- 🎨 **Syntax Highlighting**: Supports multiple languages including Rust, Python, JavaScript, Go, and more.
- 🔒 **Privacy**: Customizable token lengths and content limits.
- 🐳 **Docker Ready**: Easy deployment with Docker and Docker Compose.

## Getting Started

Note: The configuration files and `docker-compose.yml` are git-ignored. You will need to create them from the provided examples.

### Using Docker (Recommended)

1. Clone functionality repository:
   ```bash
   git clone https://github.com/Licheam/mayfile.git
   cd mayfile
   ```

2. **Prepare configuration files**:
   ```bash
   cp docker-compose.yml.example docker-compose.yml
   cp config/app.toml.example config/app.toml
   ```

3. Start the service:
   ```bash
   docker-compose up -d
   ```

The service will be available at `http://localhost:8080`.

### Manual Installation

Requirements:
- Rust (latest stable)
- SQLite

1. **Prepare configuration**:
   ```bash
   cp config/app.toml.example config/app.toml
   ```

2. Run the application:
   ```bash
   cargo run --release
   ```

## Configuration

Configuration is handled via `config/app.toml`. Make sure to copy `config/app.toml.example` to `config/app.toml` first. You can customize:

- **Server**: Host and port.
- **Paste**: Database path, expiration options, token lengths, and size limits.
- **I18n**: Locale file paths.

Example `config/app.toml`:

```toml
[server]
host = "0.0.0.0"
port = 8080

[paste]
db_path = "data/pastebin.db"
default_expires_secs = 86400  # 1 day
max_content_length = 1000000  # 1 MB
```

## API Endpoints

- `GET /`: Home page.
- `POST /paste`: Create a new paste.
- `GET /p/{token}`: View a paste.
- `GET /r/{token}`: View raw paste content.

## License

This project is licensed under the [MIT License](LICENSE).
