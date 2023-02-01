use serde::{Deserialize, Serialize};

pub mod index;
pub mod ingest;
pub mod query;

#[derive(thiserror::Error, Debug)]
pub enum ApiError {
    #[error("Environment Error")]
    Env(#[from] dotenvy::Error),
    #[error("Invalid Input: {0}")]
    InvalidInput(String),
    #[error("Deserialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Error while communicating to labrinth")]
    Api(#[from] reqwest::Error),
    #[error("Invalid Authentication Credentials: {0}")]
    Authentication(String),
    #[error("Clickhouse error: {0}")]
    Clickhouse(#[from] clickhouse::error::Error),
}

impl actix_web::ResponseError for ApiError {
    fn status_code(&self) -> actix_web::http::StatusCode {
        match self {
            ApiError::Env(..) => actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::InvalidInput(..) => actix_web::http::StatusCode::BAD_REQUEST,
            ApiError::Json(..) => actix_web::http::StatusCode::BAD_REQUEST,
            ApiError::Api(..) => actix_web::http::StatusCode::FAILED_DEPENDENCY,
            ApiError::Authentication(..) => actix_web::http::StatusCode::UNAUTHORIZED,
            ApiError::Clickhouse(..) => actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> actix_web::HttpResponse {
        actix_web::HttpResponse::build(self.status_code()).json(RawError {
            error: match self {
                ApiError::Env(..) => "environment_error",
                ApiError::InvalidInput(..) => "invalid_input",
                ApiError::Json(..) => "json_error",
                ApiError::Api(..) => "api_error",
                ApiError::Authentication(..) => "authentication_error",
                ApiError::Clickhouse(..) => "clickhouse_error",
            },
            description: &self.to_string(),
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct RawError<'a> {
    pub error: &'a str,
    pub description: &'a str,
}
