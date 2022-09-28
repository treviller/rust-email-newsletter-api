use actix_web::{dev::Server, web, App, HttpResponse, HttpServer};
use std::net::TcpListener;

async fn heatlh_check() -> HttpResponse {
    HttpResponse::Ok().finish()
}

#[derive(serde::Deserialize)]
struct NewsletterSubscriptionFormData {
    name: String,
    email: String,
}

async fn newsletter_subscribe(_form: web::Form<NewsletterSubscriptionFormData>) -> HttpResponse {
    HttpResponse::Ok().finish()
}

pub fn run(listener: TcpListener) -> Result<Server, std::io::Error> {
    let server = HttpServer::new(|| {
        App::new()
            .route("/health_check", web::get().to(heatlh_check))
            .route(
                "/newsletter/subscription",
                web::post().to(newsletter_subscribe),
            )
    })
    .listen(listener)?
    .run();

    Ok(server)
}
