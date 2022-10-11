use crate::helpers::spawn_app;
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

#[tokio::test]
async fn subscribe_to_newsletter_returns_200_with_valid_form_data() {
    let app = spawn_app().await;

    let body = "name=JohnDoe&email=test%40test.com";

    Mock::given(path("/send"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    let response = app.post_subscriptions(body.to_owned()).await;

    assert_eq!(200, response.status().as_u16());
}

#[tokio::test]
async fn subscriber_to_newsletter_persists_the_new_subscriber() {
    let app = spawn_app().await;

    let body = "name=JohnDoe&email=test%40test.com";

    Mock::given(path("/send"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    app.post_subscriptions(body.to_owned()).await;

    let saved = sqlx::query!("SELECT email, name, status FROM subscriptions",)
        .fetch_one(&app.connection_pool)
        .await
        .expect("Failed to fetch saved subscription.");

    assert_eq!(saved.email, "test@test.com");
    assert_eq!(saved.name, "JohnDoe");
    assert_eq!(saved.status, "pending_confirmation");
}

#[tokio::test]
async fn subscribe_to_newsletter_returns_400_when_fields_are_present_but_invalid() {
    let app = spawn_app().await;

    let test_cases = vec![
        ("name=&email=test2@test.com", "empty name"),
        ("name=JohnDoe&email=", "empty email"),
        ("name=JohnDoe&email=invalidemailcom", "invalid email"),
    ];

    for (body, test_case_description) in test_cases {
        let response = app.post_subscriptions(body.to_owned()).await;

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
    let test_cases = vec![
        ("email=test2@test.com", "missing the name"),
        ("name=JaneDoe", "missing the email"),
        ("", "missing both name and email"),
    ];

    for (invalid_body, error_message) in test_cases {
        let response = app.post_subscriptions(invalid_body.to_owned()).await;

        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not fail with 400 Bad Request when the payload was {}.",
            error_message
        );
    }
}

#[tokio::test]
async fn subcribe_sends_a_confirmation_email_for_valid_data() {
    let app = spawn_app().await;
    let body = "name=John%20Doe&email=x7iv7vqe2@mozmail.com";

    Mock::given(path("/send"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    app.post_subscriptions(body.into()).await;

    let email_request = &app.email_server.received_requests().await.unwrap()[0];
    let links = app.get_confirmation_links(email_request).await;

    assert_eq!(links.html, links.raw);
}

#[tokio::test]
async fn subscribe_fails_if_there_is_a_fatal_database_error() {
    let app = spawn_app().await;

    let body = "name=John%20Doe&email=test%40test.com";

    sqlx::query!("ALTER TABLE subscription_tokens DROP COLUMN subscription_token;")
        .execute(&app.connection_pool)
        .await
        .unwrap();

    let response = app.post_subscriptions(body.into()).await;
    assert_eq!(response.status().as_u16(), 500);
}
