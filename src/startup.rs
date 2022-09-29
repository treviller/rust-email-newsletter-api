use actix_web::{dev::Server, web, App, HttpResponse, HttpServer};
use std::net::TcpListener;

use crate::routes::{health_check::heatlh_check, subscription::newsletter_subscribe};

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
