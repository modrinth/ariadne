use actix_web::get;
use actix_web::HttpResponse;
use serde_json::json;

#[get("/")]
pub async fn index_get() -> HttpResponse {
    let data = json!({
        "name": "modrinth-ariadne",
        "version": env!("CARGO_PKG_VERSION"),
        "about": "This is an internal API for tracking creator analytics on Modrinth."
    });

    HttpResponse::Ok().json(data)
}
