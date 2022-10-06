use actix_web::{web, HttpResponse};
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    domain::{NewSubscriber, SubscriberEmail, SubscriberName},
    email_client::EmailClient,
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
    skip(form, connection_pool, email_client),
    fields(
        subscriber_email = %form.email,
        subscriber_name = %form.name,
    )
)]
pub async fn newsletter_subscribe(
    form: web::Form<NewsletterSubscriptionFormData>,
    connection_pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
) -> HttpResponse {
    let new_subscriber = match form.0.try_into() {
        Ok(subscriber) => subscriber,
        Err(_) => return HttpResponse::BadRequest().finish(),
    };

    if insert_subscriber(&connection_pool, &new_subscriber)
        .await
        .is_err()
    {
        tracing::error!("Failed to execute query :");
        return HttpResponse::InternalServerError().finish();
    } else {
        tracing::info!("New subscriber details have been saved");
    }

    if send_confirmation_email(&email_client, &new_subscriber)
        .await
        .is_err()
    {
        return HttpResponse::InternalServerError().finish();
    }

    HttpResponse::Ok().finish()
}

#[tracing::instrument(
    name = "Send a confirmation email to a new subscriber",
    skip(email_client, new_subscriber)
)]
pub async fn send_confirmation_email(
    email_client: &EmailClient,
    new_subscriber: &NewSubscriber,
) -> Result<(), reqwest::Error> {
    let confirmation_link = "https://localhost/subscriptions/confirm";

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
}

#[tracing::instrument(
    name = "Saving new subscriber details in the database",
    skip(form, connection_pool)
)]
pub async fn insert_subscriber(
    connection_pool: &PgPool,
    form: &NewSubscriber,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO subscriptions (id, email, name, subscribed_at, status) VALUES ($1, $2, $3, $4, 'pending_confirmation')",
        Uuid::new_v4(),
        form.email.as_ref(),
        form.name.as_ref(),
        Utc::now()
    )
    .execute(connection_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;

    Ok(())
}
