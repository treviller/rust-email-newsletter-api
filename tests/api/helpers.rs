use once_cell::sync::Lazy;
use reqwest::Url;
use rust_email_newsletter_api::{
    configuration::{loader::get_configuration, settings::DatabaseSettings},
    startup::{get_connection_pool, Application},
    telemetry::{get_subscriber, initialize_subscriber},
};
use sqlx::{Connection, Executor, PgConnection, PgPool};
use uuid::Uuid;
use wiremock::MockServer;

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

pub struct ConfirmationLinks {
    pub html: reqwest::Url,
    pub raw: reqwest::Url,
}

pub struct TestApp {
    pub address: String,
    pub port: u16,
    pub connection_pool: PgPool,
    pub email_server: MockServer,
}

impl TestApp {
    pub async fn post_subscriptions(&self, body: String) -> reqwest::Response {
        let client = reqwest::Client::new();

        client
            .post(format!("{}/newsletters/subscriptions", self.address))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .expect("Failed to execute request")
    }

    pub async fn post_newsletters(&self, body: serde_json::Value) -> reqwest::Response {
        let (username, password) = self.test_user().await;

        reqwest::Client::new()
            .post(&format!("{}/newsletters", &self.address))
            .basic_auth(username, Some(password))
            .json(&body)
            .send()
            .await
            .expect("Failed to execute request.")
    }

    pub fn get_confirmation_links(&self, email_request: &wiremock::Request) -> ConfirmationLinks {
        let request_body: serde_json::Value = serde_json::from_slice(&email_request.body).unwrap();

        let get_link = |s: &str| {
            let links: Vec<_> = linkify::LinkFinder::new()
                .links(s)
                .filter(|l| *l.kind() == linkify::LinkKind::Url)
                .collect();

            assert_eq!(links.len(), 1);

            let raw_link = links[0].as_str().to_owned();
            let mut confirmation_link = Url::parse(&raw_link).unwrap();

            assert_eq!(confirmation_link.host_str().unwrap(), "127.0.0.1");

            confirmation_link.set_port(Some(self.port)).unwrap();

            confirmation_link
        };

        ConfirmationLinks {
            html: get_link(&request_body["Html-part"].as_str().unwrap()),
            raw: get_link(&request_body["Text-part"].as_str().unwrap()),
        }
    }

    pub async fn test_user(&self) -> (String, String) {
        let row = sqlx::query!("SELECT username, password FROM users LIMIT 1",)
            .fetch_one(&self.connection_pool)
            .await
            .expect("Failed to fetch test user");

        (row.username, row.password)
    }
}

pub async fn spawn_app() -> TestApp {
    Lazy::force(&TRACING);

    let email_server = MockServer::start().await;

    let configuration = {
        let mut c = get_configuration().expect("Failed to read configuration");

        c.database.database_name = Uuid::new_v4().to_string();
        c.email_client.timeout_seconds = 1;
        c.email_client.base_url = email_server.uri();
        c.application.port = 0;

        c
    };

    configure_database(&configuration.database).await;

    let application =
        Application::build(configuration.clone()).expect("Failed to build the application");

    let port = application.port();
    let address = format!("http://127.0.0.1:{}", port);

    let _ = tokio::spawn(application.run_until_stopped());

    let test_app = TestApp {
        address,
        port,
        connection_pool: get_connection_pool(&configuration.database),
        email_server,
    };

    add_test_user(&test_app.connection_pool).await;

    test_app
}

async fn add_test_user(connection_pool: &PgPool) {
    sqlx::query!(
        "INSERT INTO users (user_id, username, password) VALUES ($1, $2, $3)",
        Uuid::new_v4(),
        Uuid::new_v4().to_string(),
        Uuid::new_v4().to_string(),
    )
    .execute(connection_pool)
    .await
    .expect("Failed to create test users.");
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
