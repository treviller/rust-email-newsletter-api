FROM lukemathwalker/cargo-chef:latest-rust-1.63.0 AS chef

WORKDIR /app
RUN apt update && apt install lld clang -y

FROM chef as planner
COPY . .
#Compute a lock-like file for our project
RUN cargo chef prepare --recipe-path recipe.json

FROM chef as builder
COPY --from=planner /app/recipe.json recipe.json
#Build project's dependencies
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .
ENV SQLX_OFFLINE true
RUN cargo build --release --bin rust_email_newsletter_api

FROM debian:bullseye-slim AS runtime

WORKDIR /app

RUN apt-get update -y \
    && apt-get install -y --no-install-recommends openssl ca-certificates \
    # Cleaning steps
    && apt-get autoremove -y \
    && apt-get clean -y \ 
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/rust_email_newsletter_api rust_email_newsletter_api
COPY configuration configuration
ENV APP_ENVIRONMENT production
ENTRYPOINT ["./rust_email_newsletter_api"]
