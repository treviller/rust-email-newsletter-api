use crate::helpers::spawn_app;

#[tokio::test]
async fn subscribe_to_newsletter_returns_200_with_valid_form_data() {
    let app = spawn_app().await;

    let body = "name=JohnDoe&email=test%40test.com";
    let response = app.post_subscriptions(body.to_owned()).await;

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
    let client = reqwest::Client::new();

    let url = format!("{}/newsletter/subscription", app.address);
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
