use actix_web::{web, HttpResponse};

#[derive(serde::Deserialize)]
pub struct NewsletterSubscriptionFormData {
    name: String,
    email: String,
}

pub async fn newsletter_subscribe(
    _form: web::Form<NewsletterSubscriptionFormData>,
) -> HttpResponse {
    HttpResponse::Ok().finish()
}
