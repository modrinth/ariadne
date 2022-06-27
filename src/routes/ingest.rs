use crate::guards::admin_key_guard;
use crate::routes::ApiError;
use crate::AnalyticsQueue;
use actix_web::HttpResponse;
use actix_web::{post, web};
use serde::Deserialize;
use std::sync::Arc;
use url::Url;

#[derive(Deserialize)]
pub struct UrlInput {
    site_path: String,
}

//Internal (can only be called with key) - protections are lax
//called from labrinth- URLs guaranteed to be valid
#[post("v1/downloads", guard = "admin_key_guard")]
pub async fn downloads_ingest(
    analytics_queue: web::Data<Arc<AnalyticsQueue>>,
    url_input: web::Json<UrlInput>,
) -> Result<HttpResponse, ApiError> {
    let url = Url::parse(&url_input.site_path)
        .map_err(|_| ApiError::InvalidInput("invalid download URL specified!".to_string()))?;

    let mut segments = url
        .path_segments()
        .ok_or_else(|| ApiError::InvalidInput("invalid download URL specified!".to_string()))?;

    let id = segments
        .nth(1)
        .ok_or_else(|| ApiError::InvalidInput("invalid download URL specified!".to_string()))?;

    if id.len() < 8 || id.len() > 11 {
        return Err(ApiError::InvalidInput(
            "invalid project ID in download URL!".to_string(),
        ));
    }

    analytics_queue
        .add_download(id.to_string(), url.to_string())
        .await;

    Ok(HttpResponse::NoContent().body(""))
}

#[derive(Deserialize)]
pub struct RevenueInput {
    project_id: String,
    revenue: f32,
}

//Internal (can only be called with key) - protections are lax
//called from ads payouts provider. TODO: figure out how to record this
#[post("v1/revenue", guard = "admin_key_guard")]
pub async fn revenue_ingest(
    analytics_queue: web::Data<Arc<AnalyticsQueue>>,
    revenue_input: web::Json<RevenueInput>,
) -> Result<HttpResponse, ApiError> {
    if revenue_input.project_id.len() < 8 || revenue_input.project_id.len() > 11 {
        return Err(ApiError::InvalidInput(
            "invalid project ID in download URL!".to_string(),
        ));
    }

    if revenue_input.revenue > 5.0 {
        return Err(ApiError::InvalidInput(
            "revenue exceeds individual request allowance!".to_string(),
        ));
    }

    analytics_queue
        .add_revenue(revenue_input.project_id.clone(), revenue_input.revenue)
        .await;

    Ok(HttpResponse::NoContent().body(""))
}

//TODO: implement ingest of page views with validation + spam protection
