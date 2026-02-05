FROM rust:trixie AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /app/target/release/mayfile /app/mayfile
COPY --from=builder /app/assets /app/assets
COPY --from=builder /app/templates /app/templates
COPY --from=builder /app/config /app/config
COPY --from=builder /app/locales /app/locales
EXPOSE 3000
CMD ["sh", "-c", "[ -f config/app.toml ] || cp config/app.toml.example config/app.toml; ./mayfile"]
