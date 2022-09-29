use std::net::TcpListener;

use rust_email_newsletter_api::configuration::get_configuration;
use sqlx::{Connection, PgConnection};

fn spawn_app() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind at random port");
    let port = listener.local_addr().unwrap().port();

    let server =
        rust_email_newsletter_api::startup::run(listener).expect("Failed to start the server");
    let _ = tokio::spawn(server);

    format!("http://127.0.0.1:{}", port)
}

#[tokio::test]
async fn health_check_works() {
    let address = spawn_app();
    let client = reqwest::Client::new();

    let url = format!("{}/health_check", address);

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
    let address = spawn_app();
    let client = reqwest::Client::new();

    let configuration = get_configuration().expect("Failed to read configuration");
    let connection_string = configuration.database.connection_string();
    let mut connection = PgConnection::connect(&connection_string)
        .await
        .expect("Failed to connect to Postgres");

    let url = format!("{}/newsletter/subscription", address);

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
        .fetch_one(&mut connection)
        .await
        .expect("Failed to fetch saved subscription.");

    assert_eq!(saved.email, "test@test.com");
    assert_eq!(saved.name, "JohnDoe");
}

#[tokio::test]
async fn subscribe_to_newsletter_returns_400_when_data_is_missing() {
    let address = spawn_app();
    let client = reqwest::Client::new();

    let url = format!("{}/newsletter/subscription", address);
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
