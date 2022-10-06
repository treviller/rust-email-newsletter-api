use actix_web::{web, HttpResponse};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(serde::Deserialize)]
pub struct Parameters {
    subscription_token: String,
}

#[tracing::instrument(
    name = "Confirm a pending subscriber",
    skip(parameters, connection_pool)
)]
pub async fn newsletter_subscription_confirm(
    parameters: web::Query<Parameters>,
    connection_pool: web::Data<PgPool>,
) -> HttpResponse {
    let subscriber_id = match get_subscriber_id_from_token(
        &connection_pool,
        &parameters.subscription_token,
    )
    .await
    {
        Ok(subscriber_id) => subscriber_id,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    match subscriber_id {
        None => HttpResponse::Unauthorized().finish(),
        Some(subscriber_id) => {
            if confirm_subscriber(&connection_pool, subscriber_id)
                .await
                .is_err()
            {
                return HttpResponse::InternalServerError().finish();
            }

            return HttpResponse::Ok().finish();
        }
    };

    HttpResponse::Ok().finish()
}

#[tracing::instrument(
    name = "Get subcriber_id from token",
    skip(connection_pool, subscription_token)
)]
async fn get_subscriber_id_from_token(
    connection_pool: &PgPool,
    subscription_token: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    let result = sqlx::query!(
        "SELECT subscriber_id FROM subscription_tokens WHERE subscription_token = $1",
        subscription_token
    )
    .fetch_optional(connection_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query : {:?}", e);
        e
    })?;

    Ok(result.map(|r| r.subscriber_id))
}

#[tracing::instrument(
    name = "Mark subscriber as confirmed",
    skip(connection_pool, subscriber_id)
)]
async fn confirm_subscriber(
    connection_pool: &PgPool,
    subscriber_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE subscriptions SET status = 'confirmed' WHERE id = $1",
        subscriber_id
    )
    .execute(connection_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query : {:?}", e);
        e
    })?;

    Ok(())
}
