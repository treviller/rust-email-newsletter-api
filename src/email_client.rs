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
    ) -> Self {
        Self {
            http_client: Client::new(),
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
            .await?;
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
    use wiremock::{
        matchers::{header, header_exists, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use super::EmailClient;
    use crate::domain::SubscriberEmail;

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
        let sender = SubscriberEmail::parse(SafeEmail().fake()).unwrap();

        Mock::given(header_exists("Authorization"))
            .and(header("Content-Type", "application/json"))
            .and(path("/send"))
            .and(method("POST"))
            .and(SendEmailBodyMatcher)
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        let email_client = EmailClient::new(
            mock_server.uri(),
            sender,
            Secret::new(Faker.fake()),
            Secret::new(Faker.fake()),
        );
        let subscriber_email = SubscriberEmail::parse(SafeEmail().fake()).unwrap();
        let subject: String = Sentence(1..2).fake();
        let content: String = Paragraph(1..10).fake();

        let _ = email_client
            .send_email(&subscriber_email, &subject, &content, &content)
            .await;
    }
}
