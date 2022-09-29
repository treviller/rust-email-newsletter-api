use actix_web::HttpResponse;

pub async fn heatlh_check() -> HttpResponse {
    HttpResponse::Ok().finish()
}
