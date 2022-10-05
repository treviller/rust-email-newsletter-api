use actix_web::{web, HttpResponse};
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::{NewSubscriber, SubscriberEmail, SubscriberName};

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
    skip(form, connection_pool),
    fields(
        subscriber_email = %form.email,
        subscriber_name = %form.name,
    )
)]
pub async fn newsletter_subscribe(
    form: web::Form<NewsletterSubscriptionFormData>,
    connection_pool: web::Data<PgPool>,
) -> HttpResponse {
    let new_subscriber = match form.0.try_into() {
        Ok(subscriber) => subscriber,
        Err(_) => return HttpResponse::BadRequest().finish(),
    };

    match insert_subscriber(&connection_pool, &new_subscriber).await {
        Ok(_) => {
            tracing::info!("New subscriber details have been saved");
            HttpResponse::Ok().finish()
        }
        Err(e) => {
            tracing::error!("Failed to execute query : {}", e);
            HttpResponse::InternalServerError().finish()
        }
    }
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
        "INSERT INTO subscriptions (id, email, name, subscribed_at, status) VALUES ($1, $2, $3, $4, 'confirmed')",
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
