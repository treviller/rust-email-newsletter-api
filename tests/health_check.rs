use std::net::TcpListener;

use once_cell::sync::Lazy;
use rust_email_newsletter_api::{
    configuration::{loader::get_configuration, settings::DatabaseSettings},
    email_client::EmailClient,
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

async fn spawn_app() -> TestApp {
    Lazy::force(&TRACING);

    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind at random port");
    let port = listener.local_addr().unwrap().port();

    let mut configuration = get_configuration().expect("Failed to read configuration");
    configuration.database.database_name = Uuid::new_v4().to_string();
    let connection_pool = configure_database(&configuration.database).await;

    let sender_email = configuration
        .email_client
        .sender()
        .expect("Invalid sender email address");
    let email_client = EmailClient::new(
        configuration.email_client.base_url,
        sender_email,
        configuration.email_client.api_key,
        configuration.email_client.secret_key,
    );

    let server =
        rust_email_newsletter_api::startup::run(listener, connection_pool.clone(), email_client)
            .expect("Failed to start the server");
    let _ = tokio::spawn(server);

    TestApp {
        address: format!("http://127.0.0.1:{}", port),
        connection_pool,
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

#[tokio::test]
async fn health_check_works() {
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    let url = format!("{}/health_check", app.address);

    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to execute request.");

    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
}

#[tokio::test]
async fn subscribe_to_newsletter_returns_200_with_valid_form_data() {
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    let url = format!("{}/newsletter/subscription", app.address);

    let body = "name=JohnDoe&email=test%40test.com";
    let response = client
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .expect("Failed to execute request");

    assert_eq!(200, response.status().as_u16());

    let saved = sqlx::query!("SELECT email, name FROM subscriptions",)
        .fetch_one(&app.connection_pool)
        .await
        .expect("Failed to fetch saved subscription.");

    assert_eq!(saved.email, "test@test.com");
    assert_eq!(saved.name, "JohnDoe");
}

#[tokio::test]
async fn subscribe_to_newsletter_returns_400_when_fields_are_present_but_invalid() {
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    let test_cases = vec![
        ("name=&email=test2@test.com", "empty name"),
        ("name=JohnDoe&email=", "empty email"),
        ("name=JohnDoe&email=invalidemailcom", "invalid email"),
    ];

    for (body, test_case_description) in test_cases {
        let response = client
            .post(&format!("{}/newsletter/subscription", &app.address))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .expect("Failed to execute request.");

        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not return a 400 Bad request when the payload was {}",
            test_case_description
        )
    }
}

#[tokio::test]
async fn subscribe_to_newsletter_returns_400_when_data_is_missing() {
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    let url = format!("{}/newsletter/subscription", app.address);
    let test_cases = vec![
        ("email=test2@test.com", "missing the name"),
        ("name=JaneDoe", "missing the email"),
        ("", "missing both name and email"),
    ];

    for (invalid_body, error_message) in test_cases {
        let response = client
            .post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(invalid_body)
            .send()
            .await
            .expect("Failed to execute request.");

        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not fail with 400 Bad Request when the payload was {}.",
            error_message
        );
    }
}
