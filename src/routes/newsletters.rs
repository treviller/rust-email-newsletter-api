use crate::domain::SubscriberEmail;
use crate::email_client::EmailClient;
use actix_web::http::StatusCode;
use actix_web::{web, HttpResponse, ResponseError};
use anyhow::Context;
use sqlx::PgPool;

#[derive(serde::Deserialize)]
pub struct PublishNewsletterBodyData {
    title: String,
    content: Content,
}

#[derive(serde::Deserialize)]
pub struct Content {
    html: String,
    text: String,
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

#[derive(thiserror::Error)]
pub enum PublishNewsletterError {
    #[error("transparent")]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for PublishNewsletterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for PublishNewsletterError {
    fn status_code(&self) -> StatusCode {
        match self {
            PublishNewsletterError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[tracing::instrument(name = "Publish a newsletter", skip(body, db_pool, email_client))]
pub async fn publish_newsletter(
    body: web::Json<PublishNewsletterBodyData>,
    db_pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
) -> Result<HttpResponse, PublishNewsletterError> {
    let subscribers = get_confirmed_subscribers(&db_pool).await?;

    for subscriber in subscribers {
        email_client
            .send_email(
                &subscriber.email,
                &body.title,
                &body.content.html,
                &body.content.text,
            )
            .await
            .with_context(|| {
                format!(
                    "Failed to send newsletter issue to {}",
                    subscriber.email.as_ref()
                )
            })?;
    }

    Ok(HttpResponse::Ok().finish())
}

pub struct ConfirmedSubscriber {
    email: SubscriberEmail,
}

#[tracing::instrument(name = "Get confirmed subscribers", skip(db_pool))]
async fn get_confirmed_subscribers(
    db_pool: &PgPool,
) -> Result<Vec<ConfirmedSubscriber>, anyhow::Error> {
    struct Row {
        email: String,
    }

    let rows: Vec<Row> = sqlx::query_as!(
        Row,
        "SELECT email FROM subscriptions WHERE status = 'confirmed'"
    )
    .fetch_all(db_pool)
    .await?;

    let confirmed_subscribers = rows
        .into_iter()
        .filter_map(|r| match SubscriberEmail::parse(r.email) {
            Ok(email) => Some(ConfirmedSubscriber { email }),
            Err(error) => {
                tracing::warn!(
                    "A confirmed subscriber is using an invalid email address. \n{}",
                    error
                );
                None
            }
        })
        .collect();

    Ok(confirmed_subscribers)
}
