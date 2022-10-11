use actix_web::{http::StatusCode, web, HttpResponse, ResponseError};
use anyhow::Context;
use chrono::Utc;
use rand::{distributions, thread_rng, Rng};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    domain::{NewSubscriber, SubscriberEmail, SubscriberName},
    email_client::EmailClient,
    startup::ApplicationBaseUrl,
};

#[derive(serde::Deserialize)]
pub struct NewsletterSubscriptionFormData {
    name: String,
    email: String,
}

impl TryFrom<NewsletterSubscriptionFormData> for NewSubscriber {
    type Error = String;

    fn try_from(value: NewsletterSubscriptionFormData) -> Result<Self, Self::Error> {
        let name = SubscriberName::parse(value.name)?;
        let email = SubscriberEmail::parse(value.email)?;

        Ok(Self { email, name })
    }
}

fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    writeln!(f, "{}\n", e)?;

    let mut current = e.source();

    while let Some(cause) = current {
        writeln!(f, "Caused by : \n\t{}", cause)?;
        current = cause.source();
    }

    Ok(())
}

pub struct StoreTokenError(sqlx::Error);

impl std::fmt::Display for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "A database error occurred while trying to store a subscription token."
        )
    }
}

impl std::fmt::Debug for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl std::error::Error for StoreTokenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

#[derive(thiserror::Error)]
pub enum SubscribeError {
    #[error("{0}")]
    ValidationError(String),
    #[error("transparent")]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for SubscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for SubscribeError {
    fn status_code(&self) -> StatusCode {
        match self {
            SubscribeError::ValidationError(_) => StatusCode::BAD_REQUEST,
            SubscribeError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[tracing::instrument(
name = "Adding a new subscriber",
skip(form, connection_pool, email_client, base_url),
fields(
subscriber_email = % form.email,
subscriber_name = % form.name,
)
)]
pub async fn newsletter_subscribe(
    form: web::Form<NewsletterSubscriptionFormData>,
    connection_pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    base_url: web::Data<ApplicationBaseUrl>,
) -> Result<HttpResponse, SubscribeError> {
    let new_subscriber = form.0.try_into().map_err(SubscribeError::ValidationError)?;
    let mut transaction = connection_pool
        .begin()
        .await
        .context("Failed to acquire a Postgres connection from the pool.")?;

    let subscriber_id = insert_subscriber(&mut transaction, &new_subscriber)
        .await
        .context("Failed to insert new subscriber in the database.")?;
    let subscription_token = generate_subscription_token();

    insert_subscription_token(&mut transaction, &subscriber_id, &subscription_token)
        .await
        .context("Failed to store the confirmation token for a new subscriber.")?;

    transaction
        .commit()
        .await
        .context("Failed to commit SQL transaction to store a new subscriber.")?;

    send_confirmation_email(
        &email_client,
        &new_subscriber,
        &base_url.0,
        &subscription_token,
    )
    .await
    .context("Failed to send a confirmation email.")?;

    Ok(HttpResponse::Ok().finish())
}

#[tracing::instrument(
    name = "Send a confirmation email to a new subscriber",
    skip(email_client, new_subscriber, base_url)
)]
pub async fn send_confirmation_email(
    email_client: &EmailClient,
    new_subscriber: &NewSubscriber,
    base_url: &str,
    confirmation_token: &str,
) -> Result<(), reqwest::Error> {
    let confirmation_link = format!(
        "{}/newsletter/subscriptions/confirm?subscription_token={}",
        base_url, confirmation_token
    );

    email_client
        .send_email(
            &new_subscriber.email,
            "Welcome",
            &format!(
                "Welcome to our newsletter ! <br/> \
            Click <a href=\"{}\">here</a> to confirm your subscription",
                confirmation_link
            ),
            &format!(
                "Welcome to our newsletter ! Visit {} to confirm your subscription.",
                confirmation_link
            ),
        )
        .await
        .map_err(|e| {
            tracing::error!("An error occurred during email sending : {:?}", e);
            e
        })
}

#[tracing::instrument(
    name = "Saving new subscriber details in the database",
    skip(form, transaction)
)]
pub async fn insert_subscriber(
    transaction: &mut Transaction<'_, Postgres>,
    form: &NewSubscriber,
) -> Result<Uuid, sqlx::Error> {
    let subscriber_id = Uuid::new_v4();

    sqlx::query!(
        "INSERT INTO subscriptions (id, email, name, subscribed_at, status) VALUES ($1, $2, $3, $4, 'pending_confirmation')",
        subscriber_id,
        form.email.as_ref(),
        form.name.as_ref(),
        Utc::now()
    )
        .execute(transaction)
        .await?;

    Ok(subscriber_id)
}

#[tracing::instrument(
    name = "Store the subscription token in the database",
    skip(transaction)
)]
async fn insert_subscription_token(
    transaction: &mut Transaction<'_, Postgres>,
    subscriber_id: &Uuid,
    subscription_token: &str,
) -> Result<(), StoreTokenError> {
    sqlx::query!(
        "INSERT INTO subscription_tokens (subscription_token, subscriber_id) VALUES ($1, $2)",
        subscription_token,
        subscriber_id
    )
    .execute(transaction)
    .await
    .map_err(StoreTokenError)?;

    Ok(())
}

fn generate_subscription_token() -> String {
    let mut rng = thread_rng();
    std::iter::repeat_with(|| rng.sample(distributions::Alphanumeric))
        .map(char::from)
        .take(25)
        .collect()
}
