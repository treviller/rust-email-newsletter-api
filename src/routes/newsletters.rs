use crate::domain::SubscriberEmail;
use crate::email_client::EmailClient;
use crate::telemetry::spawn_blocking_with_tracing;
use actix_web::body::BoxBody;
use actix_web::http::header::{HeaderMap, HeaderValue};
use actix_web::http::{header, StatusCode};
use actix_web::{web, HttpRequest, HttpResponse, ResponseError};
use anyhow::Context;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use secrecy::ExposeSecret;
use secrecy::Secret;
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
    #[error("Authentication failed.")]
    AuthenticationError(#[source] anyhow::Error),
    #[error("transparent")]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for PublishNewsletterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for PublishNewsletterError {
    fn error_response(&self) -> HttpResponse<BoxBody> {
        match self {
            PublishNewsletterError::AuthenticationError(_) => {
                let mut response = HttpResponse::new(StatusCode::UNAUTHORIZED);

                let header_value = HeaderValue::from_str(r#"Basic realm="publish""#).unwrap();

                response
                    .headers_mut()
                    .insert(header::WWW_AUTHENTICATE, header_value);

                response
            }
            PublishNewsletterError::UnexpectedError(_) => {
                HttpResponse::new(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

#[tracing::instrument(
name = "Publish a newsletter",
skip(body, db_pool, email_client, request),
fields(username = tracing::field::Empty, user_id = tracing::field::Empty)
)]
pub async fn publish_newsletter(
    request: HttpRequest,
    body: web::Json<PublishNewsletterBodyData>,
    db_pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
) -> Result<HttpResponse, PublishNewsletterError> {
    let credentials = basic_authentication(request.headers())
        .map_err(PublishNewsletterError::AuthenticationError)?;

    tracing::Span::current().record("username", &tracing::field::display(&credentials.username));

    let user_id = validate_credentials(credentials, &db_pool).await?;

    tracing::Span::current().record("user_id", &tracing::field::display(&user_id));

    let subscribers = get_confirmed_subscribers(&db_pool).await?;

    for subscriber in subscribers {
        match subscriber {
            Ok(subscriber) => {
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
            Err(error) => {
                tracing::warn!(
                    error.cause_chain = ?error,
                    "Skipping a confirmed subscriber. \
                     Their stored contact details are invalid",
                );
            }
        }
    }

    Ok(HttpResponse::Ok().finish())
}

#[tracing::instrument(
    name = "Try to find user id from credentials",
    skip(credentials, db_pool)
)]
async fn validate_credentials(
    credentials: Credentials,
    db_pool: &PgPool,
) -> Result<uuid::Uuid, PublishNewsletterError> {
    let mut user_id = None;
    let mut expected_password_hash = Secret::new(
        "$argon2id$v=19$m=15000,t=2,p=1$\
                                                 gZiV/M1gPc22ElAH/Jh1Hw$\
                                                 CWOrkoo7oJBQ/iyh7uJ0LO2aLEfrHwTWllSAxT0zRno"
            .to_string(),
    );

    if let Some((stored_user_id, stored_password_hash)) =
        get_stored_credentials(&credentials.username, &db_pool)
            .await
            .map_err(PublishNewsletterError::UnexpectedError)?
    {
        user_id = Some(stored_user_id);
        expected_password_hash = stored_password_hash;
    };

    spawn_blocking_with_tracing(move || {
        verify_password_hash(expected_password_hash, credentials.password)
    })
    .await
    .context("Failed to spawn blocking task.")
    .map_err(PublishNewsletterError::UnexpectedError)??;

    user_id.ok_or_else(|| {
        PublishNewsletterError::AuthenticationError(anyhow::anyhow!("Unknown username."))
    })
}

#[tracing::instrument(
    name = "Verify password hash",
    skip(expected_password_hash, password_candidate)
)]
fn verify_password_hash(
    expected_password_hash: Secret<String>,
    password_candidate: Secret<String>,
) -> Result<(), PublishNewsletterError> {
    let expected_password_hash = PasswordHash::new(&expected_password_hash.expose_secret())
        .context("Failed to parse hash in PHC string format")
        .map_err(PublishNewsletterError::UnexpectedError)?;

    Argon2::default()
        .verify_password(
            password_candidate.expose_secret().as_bytes(),
            &expected_password_hash,
        )
        .context("Invalid password.")
        .map_err(PublishNewsletterError::AuthenticationError)
}

#[tracing::instrument(name = "Get stored credentials", skip(username, db_pool))]
async fn get_stored_credentials(
    username: &str,
    db_pool: &PgPool,
) -> Result<Option<(uuid::Uuid, Secret<String>)>, anyhow::Error> {
    let row = sqlx::query!(
        "SELECT user_id, password_hash FROM users WHERE username = $1",
        username
    )
    .fetch_optional(db_pool)
    .await
    .context("Failed to perform a query to retrieve stored credentials")?
    .map(|row| (row.user_id, Secret::new(row.password_hash)));

    Ok(row)
}

struct Credentials {
    username: String,
    password: Secret<String>,
}

fn basic_authentication(headers: &HeaderMap) -> Result<Credentials, anyhow::Error> {
    let header_value = headers
        .get("Authorization")
        .context("The 'Authorization' header was missing")?
        .to_str()
        .context("The 'Authorization' header was not a valid UTF8 string.")?;

    let base64encoded = header_value
        .strip_prefix("Basic ")
        .context("The authorization scheme was not 'Basic'.")?;

    let decoded_bytes = base64::decode_config(base64encoded, base64::STANDARD)
        .context("Failed to base64-decode 'Basic' credentials.")?;

    let decoded_credentials = String::from_utf8(decoded_bytes)
        .context("The decoded credential string is not valid UTF8.")?;

    let mut credentials = decoded_credentials.splitn(2, ':');
    let username = credentials
        .next()
        .ok_or_else(|| anyhow::anyhow!("A username must be provided in 'Basic' auth"))?
        .to_string();

    let password = credentials
        .next()
        .ok_or_else(|| anyhow::anyhow!("A password must be provided in 'Basic' auth."))?
        .to_string();

    Ok(Credentials {
        username,
        password: Secret::new(password),
    })
}

pub struct ConfirmedSubscriber {
    email: SubscriberEmail,
}

#[tracing::instrument(name = "Get confirmed subscribers", skip(db_pool))]
async fn get_confirmed_subscribers(
    db_pool: &PgPool,
) -> Result<Vec<Result<ConfirmedSubscriber, anyhow::Error>>, anyhow::Error> {
    let confirmed_subscribers =
        sqlx::query!("SELECT email FROM subscriptions WHERE status = 'confirmed'")
            .fetch_all(db_pool)
            .await?
            .into_iter()
            .map(|r| match SubscriberEmail::parse(r.email) {
                Ok(email) => Ok(ConfirmedSubscriber { email }),
                Err(error) => Err(anyhow::anyhow!(error)),
            })
            .collect();

    Ok(confirmed_subscribers)
}
