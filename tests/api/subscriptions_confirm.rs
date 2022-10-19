use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

use crate::helpers::spawn_app;

#[tokio::test]
async fn confirmations_without_token_are_rejected_with_a_400() {
    let app = spawn_app().await;

    let response = reqwest::get(&format!(
        "{}/newsletters/subscriptions/confirm",
        app.address
    ))
    .await
    .unwrap();

    assert_eq!(response.status().as_u16(), 400);
}

#[tokio::test]
async fn the_link_returned_by_subscribe_returns_a_200_if_called() {
    let app = spawn_app().await;

    let body = "name=John%20Doe&email=x7iv7vqe2@mozmail.com";

    Mock::given(path("/send"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.email_server)
        .await;

    app.post_subscriptions(body.into()).await;

    let email_request = &app.email_server.received_requests().await.unwrap()[0];
    let links = app.get_confirmation_links(email_request);
    let response = reqwest::get(links.html).await.unwrap();

    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn clicking_on_the_confirmation_link_confirms_a_subscriber() {
    let app = spawn_app().await;

    let body = "name=John%20Doe&email=x7iv7vqe2@mozmail.com";

    Mock::given(path("/send"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.email_server)
        .await;

    app.post_subscriptions(body.into()).await;
    let email_request = &app.email_server.received_requests().await.unwrap()[0];
    let links = app.get_confirmation_links(email_request);

    reqwest::get(links.html)
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let saved = sqlx::query!("SELECT email, name, status FROM subscriptions")
        .fetch_one(&app.connection_pool)
        .await
        .expect("Failed to fetch saved subscription");

    assert_eq!(saved.email, "x7iv7vqe2@mozmail.com");
    assert_eq!(saved.name, "John Doe");
    assert_eq!(saved.status, "confirmed");
}
