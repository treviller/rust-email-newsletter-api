use reqwest::Client;
use secrecy::{ExposeSecret, Secret};
use serde::{ser::SerializeStruct, Serialize};

use crate::domain::SubscriberEmail;

pub struct EmailClient {
    sender: SubscriberEmail,
    base_url: String,
    http_client: Client,
    api_key: Secret<String>,
    secret_key: Secret<String>,
}

impl EmailClient {
    pub fn new(
        base_url: String,
        sender: SubscriberEmail,
        api_key: Secret<String>,
        secret_key: Secret<String>,
        timeout: std::time::Duration,
    ) -> Self {
        Self {
            http_client: Client::builder().timeout(timeout).build().unwrap(),
            base_url,
            sender,
            api_key,
            secret_key,
        }
    }

    pub async fn send_email(
        &self,
        recipient_email: &SubscriberEmail,
        subject: &str,
        html_content: &str,
        text_content: &str,
    ) -> Result<(), reqwest::Error> {
        let url = format!("{}/send", self.base_url);
        let request_body = SendEmailRequest {
            from_email: self.sender.as_ref(),
            to: recipient_email.as_ref(),
            subject,
            html_part: html_content,
            text_part: text_content,
        };

        self.http_client
            .post(&url)
            .basic_auth(
                &self.api_key.expose_secret(),
                Some(&self.secret_key.expose_secret()),
            )
            .json(&request_body)
            .send()
            .await?
            .error_for_status()?;

        Ok(())
    }
}

struct SendEmailRequest<'a> {
    from_email: &'a str,
    subject: &'a str,
    text_part: &'a str,
    html_part: &'a str,
    to: &'a str,
}

impl<'a> Serialize for SendEmailRequest<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut s = serializer.serialize_struct("SendEmailRequest", 5)?;

        s.serialize_field("FromEmail", &self.from_email)?;
        s.serialize_field("Subject", &self.subject)?;
        s.serialize_field("Html-part", &self.html_part)?;
        s.serialize_field("Text-part", &self.text_part)?;
        s.serialize_field("To", &self.to)?;

        s.end()
    }
}

#[cfg(test)]
mod tests {
    use fake::{
        faker::{
            internet::en::SafeEmail,
            lorem::en::{Paragraph, Sentence},
        },
        Fake, Faker,
    };
    use secrecy::Secret;
    use tracing_subscriber::fmt::init;
    use wiremock::{
        matchers::{header, header_exists, method, path},
        Mock, MockBuilder, MockServer, ResponseTemplate,
    };

    use super::EmailClient;
    use crate::domain::SubscriberEmail;
    use claim::{assert_err, assert_ok};

    struct SendEmailBodyMatcher;

    impl wiremock::Match for SendEmailBodyMatcher {
        fn matches(&self, request: &wiremock::Request) -> bool {
            let result: Result<serde_json::Value, _> = serde_json::from_slice(&request.body);

            if let Ok(body) = result {
                body.get("FromEmail").is_some()
                    && body.get("To").is_some()
                    && body.get("Subject").is_some()
                    && body.get("Html-part").is_some()
                    && body.get("Text-part").is_some()
            } else {
                false
            }
        }
    }

    #[tokio::test]
    async fn send_email_sends_the_expected_request() {
        let mock_server = MockServer::start().await;

        create_mock(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let email_client = init_email_client(mock_server.uri());
        let result = send_email_request(&email_client).await;

        assert_ok!(result)
    }

    #[tokio::test]
    async fn send_email_fails_if_the_api_returns_500() {
        let mock_server = MockServer::start().await;

        create_mock(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let email_client = init_email_client(mock_server.uri());
        let result = send_email_request(&email_client).await;

        assert_err!(result);
    }

    #[tokio::test]
    async fn send_email_times_out_if_the_api_takes_too_long() {
        let mock_server = MockServer::start().await;

        let mock_response =
            ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(180));

        create_mock(mock_response).mount(&mock_server).await;

        let email_client = init_email_client(mock_server.uri());
        let result = send_email_request(&email_client).await;

        assert_err!(result);
    }

    fn create_mock(response_template: ResponseTemplate) -> Mock {
        Mock::given(header_exists("Authorization"))
            .and(header("Content-Type", "application/json"))
            .and(path("/send"))
            .and(method("POST"))
            .and(SendEmailBodyMatcher)
            .respond_with(response_template)
            .expect(1)
    }

    fn init_email_client(base_url: String) -> EmailClient {
        let sender = SubscriberEmail::parse(SafeEmail().fake()).unwrap();

        EmailClient::new(
            base_url,
            sender,
            Secret::new(Faker.fake()),
            Secret::new(Faker.fake()),
            std::time::Duration::from_millis(200),
        )
    }

    async fn send_email_request(email_client: &EmailClient) -> Result<(), reqwest::Error> {
        let subscriber_email = SubscriberEmail::parse(SafeEmail().fake()).unwrap();
        let subject: String = Sentence(1..2).fake();
        let content: String = Paragraph(1..10).fake();

        email_client
            .send_email(&subscriber_email, &subject, &content, &content)
            .await
    }
}
