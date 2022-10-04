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
        recipient_email: SubscriberEmail,
        subject: &str,
        html_content: &str,
        text_content: &str,
    ) -> Result<(), reqwest::Error> {
        let url = format!("{}/send", self.base_url);
        let request_body = SendEmailRequest {
            from_email: self.sender.as_ref().to_owned(),
            to: recipient_email.as_ref().to_owned(),
            subject: subject.to_owned(),
            html_part: html_content.to_owned(),
            text_part: text_content.to_owned(),
        };

        let builder = self
            .http_client
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

struct SendEmailRequest {
    from_email: String,
    subject: String,
    text_part: String,
    html_part: String,
    to: String,
}

impl Serialize for SendEmailRequest {
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
    use wiremock::{matchers::any, Mock, MockServer, ResponseTemplate};

    use crate::domain::SubscriberEmail;

    use super::EmailClient;

    #[tokio::test]
    async fn send_email_fires_a_request_to_base_url() {
        let mock_server = MockServer::start().await;
        let sender = SubscriberEmail::parse(SafeEmail().fake()).unwrap();

        Mock::given(any())
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
            .send_email(subscriber_email, &subject, &content, &content)
            .await;
    }
}
