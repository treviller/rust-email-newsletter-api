use once_cell::sync::Lazy;
use rust_email_newsletter_api::{
    configuration::{loader::get_configuration, settings::DatabaseSettings},
    startup::{get_connection_pool, Application},
    telemetry::{get_subscriber, initialize_subscriber},
};
use sqlx::{Connection, Executor, PgConnection, PgPool};
use uuid::Uuid;

static TRACING: Lazy<()> = Lazy::new(|| {
    let default_level_filter = "info".to_string();
    let subscriber_name = "test".to_string();
    if std::env::var("TEST_LOG").is_ok() {
        let subscriber = get_subscriber(subscriber_name, default_level_filter, std::io::stdout);
        initialize_subscriber(subscriber);
    } else {
        let subscriber = get_subscriber(subscriber_name, default_level_filter, std::io::sink);
        initialize_subscriber(subscriber);
    }
});

pub struct TestApp {
    pub address: String,
    pub connection_pool: PgPool,
}

impl TestApp {
    pub async fn post_subscriptions(&self, body: String) -> reqwest::Response {
        let client = reqwest::Client::new();

        client
            .post(format!("{}/newsletter/subscription", self.address))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .expect("Failed to execute request")
    }
}

pub async fn spawn_app() -> TestApp {
    Lazy::force(&TRACING);

    let configuration = {
        let mut c = get_configuration().expect("Failed to read configuration");

        c.database.database_name = Uuid::new_v4().to_string();
        c.email_client.timeout_seconds = 1;
        c.application.port = 0;

        c
    };

    configure_database(&configuration.database).await;

    let application =
        Application::build(configuration.clone()).expect("Failed to build the application");

    let address = format!("http://127.0.0.1:{}", application.port());

    let _ = tokio::spawn(application.run_until_stopped());

    TestApp {
        address,
        connection_pool: get_connection_pool(&configuration.database),
    }
}

async fn configure_database(config: &DatabaseSettings) -> PgPool {
    let mut connection = PgConnection::connect_with(&config.without_db())
        .await
        .expect("Failed to connect to Postgres");

    connection
        .execute(format!(r#"CREATE DATABASE "{}";"#, config.database_name).as_str())
        .await
        .expect("Failed to create database");

    let connection_pool = PgPool::connect_with(config.with_db())
        .await
        .expect("Failed to connect to Postgres");

    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to migrate database");

    connection_pool
}
