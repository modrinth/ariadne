use crate::routes::ApiError;
use actix_web::{get, web, HttpRequest, HttpResponse};
use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::auth::check_is_authorized;
use crate::base62::parse_base62;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct AnalyticsQuery {
    // mandatory for non-admins
    project_id: Option<String>,
    #[serde(default = "start_interval_default")]
    start_date: DateTime<Utc>,
    #[serde(default = "Utc::now")]
    end_date: DateTime<Utc>,
}

fn start_interval_default() -> DateTime<Utc> {
    Utc::now() - chrono::Duration::weeks(1)
}

#[derive(Serialize)]
pub struct AnalyticsResponse<T> {
    pub top_values: Vec<TopValue<T>>,
    pub time_series: Vec<TimeSeriesValue<T>>,
}

#[derive(Serialize)]
pub struct TopValue<T> {
    pub value: T,
    // optional for revenue only
    pub site_path: Option<String>,
}

#[derive(Serialize)]
pub struct TimeSeriesValue<T> {
    pub value: T,
    pub recorded: DateTime<Utc>,
}

async fn perform_analytics_checks(
    query: &AnalyticsQuery,
    req: &HttpRequest,
    use_payouts_permission: bool,
) -> Result<String, ApiError> {
    check_is_authorized(
        query.project_id.as_deref(),
        req.headers(),
        use_payouts_permission,
    )
    .await?;

    let interval = (query.end_date - query.start_date).num_seconds();

    if interval < 300 {
        return Err(ApiError::InvalidInput(
            "Invalid start/end dates specified.".to_string(),
        ));
    }

    Ok(if interval > (270 * 24 * 60 * 60) {
        "month"
    } else if interval > (90 * 24 * 60 * 60) {
        "week"
    } else if interval > (30 * 24 * 60 * 60) {
        "day"
    } else if interval > (7 * 24 * 60 * 60) {
        "hour"
    } else {
        "minute"
    }
    .to_string())
}

#[get("v1/views")]
pub async fn views_query(
    web::Query(query): web::Query<AnalyticsQuery>,
    req: HttpRequest,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, ApiError> {
    let value = perform_analytics_checks(&query, &req, false).await?;

    use futures::TryStreamExt;

    let (top_values, time_series) = if let Some(project_id) = query.project_id {
        let parsed = parse_base62(&project_id)
            .map_err(|_| ApiError::InvalidInput("invalid project ID!".to_string()))?;

        futures::future::try_join(
            sqlx::query!(
                "
                SELECT SUM(views) page_views, site_path
                FROM views
                WHERE views.project_id = $3 AND (views.recorded BETWEEN $1 AND $2)
                GROUP BY site_path
                ORDER BY page_views DESC
                LIMIT 15
                ",
                query.start_date,
                query.end_date,
                parsed as i64
            )
            .fetch_many(&**pool)
            .try_filter_map(|e| async {
                Ok(e.right().map(|v| TopValue {
                    value: v.page_views.unwrap_or_default() as u32,
                    site_path: Some(v.site_path),
                }))
            })
            .try_collect::<Vec<TopValue<u32>>>(),
            sqlx::query!(
                "
                SELECT SUM(views) page_views, date_trunc($4, recorded) as recorded_date
                FROM views
                WHERE views.project_id = $3 AND (views.recorded BETWEEN $1 AND $2)
                GROUP BY recorded_date
                ORDER BY 2 ASC;
                ",
                query.start_date,
                query.end_date,
                parsed as i64,
                value
            )
            .fetch_many(&**pool)
            .try_filter_map(|e| async {
                Ok(e.right().map(|v| TimeSeriesValue {
                    value: v.page_views.unwrap_or_default() as u32,
                    recorded: v.recorded_date.unwrap_or_else(Utc::now),
                }))
            })
            .try_collect::<Vec<TimeSeriesValue<u32>>>(),
        )
        .await?
    } else {
        futures::future::try_join(
            sqlx::query!(
                "
                SELECT SUM(views) page_views, site_path
                FROM views
                WHERE (views.recorded BETWEEN $1 AND $2)
                GROUP BY site_path
                ORDER BY page_views DESC
                LIMIT 15
                ",
                query.start_date,
                query.end_date
            )
            .fetch_many(&**pool)
            .try_filter_map(|e| async {
                Ok(e.right().map(|v| TopValue {
                    value: v.page_views.unwrap_or_default() as u32,
                    site_path: Some(v.site_path),
                }))
            })
            .try_collect::<Vec<TopValue<u32>>>(),
            sqlx::query!(
                "
                SELECT SUM(views) page_views, date_trunc($3, recorded) as recorded_date
                FROM views
                WHERE (views.recorded BETWEEN $1 AND $2)
                GROUP BY recorded_date
                ORDER BY 2 ASC;
                ",
                query.start_date,
                query.end_date,
                value
            )
            .fetch_many(&**pool)
            .try_filter_map(|e| async {
                Ok(e.right().map(|v| TimeSeriesValue {
                    value: v.page_views.unwrap_or_default() as u32,
                    recorded: v.recorded_date.unwrap_or_else(Utc::now),
                }))
            })
            .try_collect::<Vec<TimeSeriesValue<u32>>>(),
        )
        .await?
    };

    Ok(HttpResponse::Ok().json(AnalyticsResponse {
        top_values,
        time_series,
    }))
}

#[get("v1/downloads")]
pub async fn downloads_query(
    web::Query(query): web::Query<AnalyticsQuery>,
    req: HttpRequest,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, ApiError> {
    let value = perform_analytics_checks(&query, &req, false).await?;

    use futures::TryStreamExt;

    let (top_values, time_series) = if let Some(project_id) = query.project_id {
        let parsed = parse_base62(&project_id)
            .map_err(|_| ApiError::InvalidInput("invalid project ID!".to_string()))?;

        futures::future::try_join(
            sqlx::query!(
                "
                SELECT SUM(downloads) downloads_value, site_path
                FROM downloads
                WHERE downloads.project_id = $3 AND (downloads.recorded BETWEEN $1 AND $2)
                GROUP BY site_path
                ORDER BY downloads_value DESC
                LIMIT 15
                ",
                query.start_date,
                query.end_date,
                parsed as i64
            )
            .fetch_many(&**pool)
            .try_filter_map(|e| async {
                Ok(e.right().map(|v| TopValue {
                    value: v.downloads_value.unwrap_or_default() as u32,
                    site_path: Some(v.site_path),
                }))
            })
            .try_collect::<Vec<TopValue<u32>>>(),
            sqlx::query!(
                "
                SELECT SUM(downloads) downloads_value, date_trunc($4, recorded) as recorded_date
                FROM downloads
                WHERE downloads.project_id = $3 AND (downloads.recorded BETWEEN $1 AND $2)
                GROUP BY recorded_date
                ORDER BY 2 ASC;
                ",
                query.start_date,
                query.end_date,
                parsed as i64,
                value
            )
            .fetch_many(&**pool)
            .try_filter_map(|e| async {
                Ok(e.right().map(|v| TimeSeriesValue {
                    value: v.downloads_value.unwrap_or_default() as u32,
                    recorded: v.recorded_date.unwrap_or_else(Utc::now),
                }))
            })
            .try_collect::<Vec<TimeSeriesValue<u32>>>(),
        )
        .await?
    } else {
        futures::future::try_join(
            sqlx::query!(
                "
                SELECT SUM(downloads) downloads_value, site_path
                FROM downloads
                WHERE (downloads.recorded BETWEEN $1 AND $2)
                GROUP BY site_path
                ORDER BY downloads_value DESC
                LIMIT 15
                ",
                query.start_date,
                query.end_date
            )
            .fetch_many(&**pool)
            .try_filter_map(|e| async {
                Ok(e.right().map(|v| TopValue {
                    value: v.downloads_value.unwrap_or_default() as u32,
                    site_path: Some(v.site_path),
                }))
            })
            .try_collect::<Vec<TopValue<u32>>>(),
            sqlx::query!(
                "
                SELECT SUM(downloads) downloads_value, date_trunc($3, recorded) as recorded_date
                FROM downloads
                WHERE (downloads.recorded BETWEEN $1 AND $2)
                GROUP BY recorded_date
                ORDER BY 2 ASC;
                ",
                query.start_date,
                query.end_date,
                value
            )
            .fetch_many(&**pool)
            .try_filter_map(|e| async {
                Ok(e.right().map(|v| TimeSeriesValue {
                    value: v.downloads_value.unwrap_or_default() as u32,
                    recorded: v.recorded_date.unwrap_or_else(Utc::now),
                }))
            })
            .try_collect::<Vec<TimeSeriesValue<u32>>>(),
        )
        .await?
    };

    Ok(HttpResponse::Ok().json(AnalyticsResponse {
        top_values,
        time_series,
    }))
}

#[get("v1/revenue")]
pub async fn revenue_query(
    web::Query(query): web::Query<AnalyticsQuery>,
    req: HttpRequest,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, ApiError> {
    let value = perform_analytics_checks(&query, &req, false).await?;

    use futures::TryStreamExt;

    let (top_values, time_series) = if let Some(project_id) = query.project_id {
        let parsed = parse_base62(&project_id)
            .map_err(|_| ApiError::InvalidInput("invalid project ID!".to_string()))?;

        (
            Vec::new(),
            sqlx::query!(
                "
                SELECT SUM(money) money_value, date_trunc($4, recorded) as recorded_date
                FROM revenue
                WHERE revenue.project_id = $3 AND (revenue.recorded BETWEEN $1 AND $2)
                GROUP BY recorded_date
                ORDER BY 2 ASC;
                ",
                query.start_date,
                query.end_date,
                parsed as i64,
                value
            )
            .fetch_many(&**pool)
            .try_filter_map(|e| async {
                Ok(e.right().map(|v| TimeSeriesValue {
                    value: v.money_value.unwrap_or_default() as f32,
                    recorded: v.recorded_date.unwrap_or_else(Utc::now),
                }))
            })
            .try_collect::<Vec<TimeSeriesValue<f32>>>()
            .await?,
        )
    } else {
        (
            Vec::new(),
            sqlx::query!(
                "
                SELECT SUM(money) money_value, date_trunc($3, recorded) as recorded_date
                FROM revenue
                WHERE (revenue.recorded BETWEEN $1 AND $2)
                GROUP BY recorded_date
                ORDER BY 2 ASC;
                ",
                query.start_date,
                query.end_date,
                value
            )
            .fetch_many(&**pool)
            .try_filter_map(|e| async {
                Ok(e.right().map(|v| TimeSeriesValue {
                    value: v.money_value.unwrap_or_default() as f32,
                    recorded: v.recorded_date.unwrap_or_else(Utc::now),
                }))
            })
            .try_collect::<Vec<TimeSeriesValue<f32>>>()
            .await?,
        )
    };

    Ok(HttpResponse::Ok().json(AnalyticsResponse {
        top_values,
        time_series,
    }))
}
