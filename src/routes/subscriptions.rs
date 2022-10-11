use actix_web::{web, HttpResponse};
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

#[tracing::instrument(
    name = "Adding a new subscriber",
    skip(form, connection_pool, email_client, base_url),
    fields(
        subscriber_email = %form.email,
        subscriber_name = %form.name,
    )
)]
pub async fn newsletter_subscribe(
    form: web::Form<NewsletterSubscriptionFormData>,
    connection_pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    base_url: web::Data<ApplicationBaseUrl>,
) -> HttpResponse {
    let new_subscriber = match form.0.try_into() {
        Ok(subscriber) => subscriber,
        Err(_) => return HttpResponse::BadRequest().finish(),
    };

    let mut transaction = match connection_pool.begin().await {
        Ok(transaction) => transaction,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    let subscriber_id = match insert_subscriber(&mut transaction, &new_subscriber).await {
        Ok(subscriber_id) => subscriber_id,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    let subscription_token = generate_subscription_token();

    if insert_subscription_token(&mut transaction, &subscriber_id, &subscription_token)
        .await
        .is_err()
    {
        return HttpResponse::InternalServerError().finish();
    }

    if send_confirmation_email(
        &email_client,
        &new_subscriber,
        &base_url.0,
        &subscription_token,
    )
    .await
    .is_err()
    {
        return HttpResponse::InternalServerError().finish();
    }

    if transaction.commit().await.is_err() {
        return HttpResponse::InternalServerError().finish();
    }

    HttpResponse::Ok().finish()
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
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;

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
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO subscription_tokens (subscription_token, subscriber_id) VALUES ($1, $2)",
        subscription_token,
        subscriber_id
    )
    .execute(transaction)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);

        e
    })?;

    Ok(())
}

fn generate_subscription_token() -> String {
    let mut rng = thread_rng();
    std::iter::repeat_with(|| rng.sample(distributions::Alphanumeric))
        .map(char::from)
        .take(25)
        .collect()
}
