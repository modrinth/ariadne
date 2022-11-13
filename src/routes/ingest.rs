use crate::base62::parse_base62;
use crate::guards::admin_key_guard;
use crate::routes::ApiError;
use crate::{AnalyticsQueue, RateLimitQueue};
use actix_web::http::header::USER_AGENT;
use actix_web::{post, web};
use actix_web::{HttpRequest, HttpResponse};
use isbot::Bots;
use serde::Deserialize;
use std::sync::Arc;
use url::Url;

#[derive(Deserialize)]
pub struct DownloadInput {
    url: String,
    project_id: String,
}

//Internal (can only be called with key) - protections are lax
//called from labrinth- URLs guaranteed to be valid
#[post("v1/downloads", guard = "admin_key_guard")]
pub async fn downloads_ingest(
    analytics_queue: web::Data<Arc<AnalyticsQueue>>,
    url_input: web::Json<DownloadInput>,
) -> Result<HttpResponse, ApiError> {
    let url = Url::parse(&url_input.url)
        .map_err(|_| ApiError::InvalidInput("invalid download URL specified!".to_string()))?;

    let parsed = parse_base62(&url_input.project_id)
        .map_err(|_| ApiError::InvalidInput("invalid project ID in download URL!".to_string()))?;

    analytics_queue
        .add_download(parsed, url.path().to_string())
        .await;

    Ok(HttpResponse::NoContent().body(""))
}

#[derive(Deserialize)]
pub struct UrlInput {
    url: String,
}

//this route should be behind the cloudflare WAF to prevent non-browsers from calling it
#[post("v1/view")]
pub async fn page_view_ingest(
    req: HttpRequest,
    rate_limit_queue: web::Data<Arc<RateLimitQueue>>,
    analytics_queue: web::Data<Arc<AnalyticsQueue>>,
    bots: web::Data<Arc<Bots>>,
    url_input: web::Json<UrlInput>,
) -> Result<HttpResponse, ApiError> {
    let admin_key = dotenv::var("ARIADNE_ADMIN_KEY")?;

    if let Some(user_agent) = req.headers().get(USER_AGENT).and_then(|x| x.to_str().ok()) {
        if bots.is_bot(user_agent) {
            return Ok(HttpResponse::NoContent().body(""));
        }
    }

    let conn_info = req.connection_info().peer_addr().map(|x| x.to_string());

    let ip = if let Some(header) = req.headers().get("CF-Connecting-IP") {
        header.to_str().ok()
    } else {
        conn_info.as_deref()
    }
    .unwrap_or_default();

    let url = Url::parse(&url_input.url)
        .map_err(|_| ApiError::InvalidInput("invalid page view URL specified!".to_string()))?;

    let domain = url
        .domain()
        .ok_or_else(|| ApiError::InvalidInput("invalid page view URL specified!".to_string()))?;

    if !(domain.ends_with(".modrinth.com") || domain == "modrinth.com") {
        return Err(ApiError::InvalidInput(
            "invalid page view URL specified!".to_string(),
        ));
    }

    if !req
        .headers()
        .get(crate::guards::ADMIN_KEY_HEADER)
        .and_then(|x| x.to_str().ok())
        .map(|x| x == &*admin_key)
        .unwrap_or_default()
        && !rate_limit_queue
            .add(ip.to_string(), url.path().to_string())
            .await
    {
        // early return, unauthorized
        return Ok(HttpResponse::NoContent().body(""));
    }

    if let Some(segments) = url.path_segments() {
        let segments_vec = segments.collect::<Vec<_>>();

        if segments_vec.len() >= 2 {
            //todo: fetch from labrinth periodically when route exists
            const PROJECT_TYPES: &[&str] = &["mod", "modpack", "plugin", "resourcepack"];

            if PROJECT_TYPES.contains(&segments_vec[0]) {
                #[derive(Deserialize)]
                struct CheckResponse {
                    id: String,
                }

                let client = reqwest::Client::new();

                let response = client
                    .get(format!(
                        "{}project/{}/check",
                        dotenv::var("LABRINTH_API_URL")?,
                        &segments_vec[1]
                    ))
                    .header("x-ratelimit-key", dotenv::var("LABRINTH_RATE_LIMIT_KEY")?)
                    .send()
                    .await?;

                if response.status().is_success() {
                    let check_response = response.json::<CheckResponse>().await?;

                    analytics_queue
                        .add_view(
                            Some(parse_base62(&check_response.id).unwrap_or_default()),
                            url.path().to_string(),
                        )
                        .await;

                    return Ok(HttpResponse::NoContent().body(""));
                }
            }
        }
    }

    analytics_queue.add_view(None, url.path().to_string()).await;

    Ok(HttpResponse::NoContent().body(""))
}
