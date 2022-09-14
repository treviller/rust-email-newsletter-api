use actix_web::{HttpServer, web, Responder, App, HttpResponse, dev::Server};
use std::net::TcpListener;

async fn heatlh_check() -> impl Responder {
    HttpResponse::Ok()
}

pub fn run(listener: TcpListener) -> Result<Server, std::io::Error> {
    let server = HttpServer::new(|| {
        App::new()
            .route("/health_check", web::get().to(heatlh_check))
    })
    .listen(listener)?
    .run();

    Ok(server)
}