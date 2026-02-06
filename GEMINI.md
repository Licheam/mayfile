# Mayfile (Pastebin Service)

Minimalist, high-performance, self-hosted pastebin service built with Rust.

## Project Overview

Mayfile is a modern pastebin application designed for speed and ease of use. It allows users to quickly share text snippets with features like expiration, burn-on-read, and syntax highlighting.

- **Backend**: Rust with [Axum](https://github.com/tokio-rs/axum) framework.
- **Database**: SQLite managed via [SQLx](https://github.com/launchbadge/sqlx).
- **Templating**: [Askama](https://github.com/djc/askama) for type-safe HTML templates.
- **Frontend**: Minimalist UI using [HTMX](https://htmx.org/) for dynamic interactions without complex JavaScript frameworks.
- **I18n**: Multi-language support (English and Chinese) via TOML locale files.

## Architecture & Features

### Core Logic (`src/main.rs`)
- **Schema Management**: Database schema is automatically checked and updated on startup in `ensure_schema`.
- **Paste Management**:
    - **Token Generation**: Short, customizable alphanumeric tokens.
    - **Expiration**: Pastes are automatically deleted after a specified time.
    - **Burn-on-Read**: Pastes can be set to delete after a certain number of views.
    - **Public/Private**: Users can choose to list pastes in the "Explore" section.
- **Cleanup**: Automatic cleanup of expired pastes happens on every index access and paste creation/view.

### Configuration
- Configuration is stored in `config/app.toml` (copy from `config/app.toml.example`).
- Supports server settings (host/port), paste constraints (limits, expiration options), and i18n paths.

### Internationalization
- Locales are defined in `locales/en.toml` and `locales/zh.toml`.
- Language is detected via `Accept-Language` header, cookies, or `?lang=` query parameter.

## Building and Running

### Development
```bash
# Copy example config
cp config/app.toml.example config/app.toml

# Run the application
cargo run
```

### Production
```bash
# Build release binary
cargo build --release

# Run release binary
./target/release/mayfile
```

### Docker
```bash
cp docker-compose.yml.example docker-compose.yml
docker-compose up -d
```

## Development Conventions

- **Database**: Use SQLx macros for compile-time checked queries.
- **Templates**: Templates are located in `templates/`. Changes require a recompile due to Askama's nature.
- **Styles**: CSS is located in `assets/style.css`.
- **Async**: Everything is async using `tokio`.
- **HTMX**: Use `hx-` attributes for AJAX-like behavior in forms and buttons.

## Important Notes

- The database file is located at the path specified in `config/app.toml` (default: `data/pastebin.db`).
- Expired pastes are physically deleted from the database to save space.
- Content length and total storage limits are enforced based on the configuration.
