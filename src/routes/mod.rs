use serde::{Deserialize, Serialize};

pub mod index;
pub mod ingest;

#[derive(thiserror::Error, Debug)]
pub enum ApiError {
    #[error("Environment Error")]
    Env(#[from] dotenv::Error),
    #[error("Database Error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Invalid Input: {0}")]
    InvalidInput(String),
    #[error("Deserialization error: {0}")]
    Json(#[from] serde_json::Error),
}

impl actix_web::ResponseError for ApiError {
    fn status_code(&self) -> actix_web::http::StatusCode {
        match self {
            ApiError::Env(..) => actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::Database(..) => actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::InvalidInput(..) => actix_web::http::StatusCode::BAD_REQUEST,
            ApiError::Json(..) => actix_web::http::StatusCode::BAD_REQUEST,
        }
    }

    fn error_response(&self) -> actix_web::HttpResponse {
        actix_web::HttpResponse::build(self.status_code()).json(RawError {
            error: match self {
                ApiError::Env(..) => "environment_error",
                ApiError::Database(..) => "database_error",
                ApiError::InvalidInput(..) => "invalid_input",
                ApiError::Json(..) => "json_error",
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
