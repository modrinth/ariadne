use std::collections::HashMap;
use crate::routes::ApiError;
use actix_web::{get, web, HttpRequest, HttpResponse};
use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;

use crate::auth::check_is_authorized;
use crate::base62::parse_base62;
use crate::guards::admin_key_guard;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Copy, Deserialize, PartialOrd, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    FiveMinutes,
    FifteenMinutes,
    OneHour,
    SixHours,
    TwelveHours,
    OneDay,
    OneWeek,
    OneMonth,
}

impl Resolution {
    pub fn convert_to_postgres(&self) -> &'static str {
        match self {
            Resolution::FiveMinutes => "1 minute",
            Resolution::FifteenMinutes => "15 minutes",
            Resolution::OneHour => "1 hour",
            Resolution::SixHours => "6 hours",
            Resolution::TwelveHours => "12 hours",
            Resolution::OneDay => "1 day",
            Resolution::OneWeek => "1 week",
            Resolution::OneMonth => "1 month",
        }
    }
}

#[derive(Deserialize)]
pub struct AnalyticsQuery {
    // mandatory for non-admins
    project_id: Option<String>,
    #[serde(default = "start_interval_default")]
    start_date: DateTime<Utc>,
    #[serde(default = "Utc::now")]
    end_date: DateTime<Utc>,
    resolution: Option<Resolution>,
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
) -> Result<&'static str, ApiError> {
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

    let min_resolution = if interval > (2 * 365 * 24 * 60 * 60) {
        Resolution::OneWeek
    } else if interval > (90 * 24 * 60 * 60) {
        Resolution::OneDay
    } else if interval > (30 * 24 * 60 * 60) {
        Resolution::OneHour
    } else if interval > (7 * 24 * 60 * 60) {
        Resolution::FifteenMinutes
    } else {
        Resolution::FiveMinutes
    };

    Ok(if let Some(resolution) = &query.resolution {
        if resolution < &min_resolution {
            min_resolution
        } else {
            *resolution
        }
    } else {
        min_resolution
    }
    .convert_to_postgres())
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
                SELECT SUM(views) page_views, date_bin($4::text::interval, recorded, TIMESTAMP '2001-01-01') as recorded_date
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
                SELECT SUM(views) page_views, date_bin($3::text::interval, recorded, TIMESTAMP '2001-01-01') as recorded_date
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

#[get("v1/downloads", guard = "admin_key_guard")]
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
                SELECT SUM(downloads) downloads_value, date_bin($4::text::interval, recorded, TIMESTAMP '2001-01-01') as recorded_date
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
                SELECT SUM(downloads) downloads_value, date_bin($3::text::interval, recorded, TIMESTAMP '2001-01-01') as recorded_date
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

#[derive(Deserialize)]
pub struct MultipliersQuery {
    start_date: DateTime<Utc>,
}

/// Internal route - retrieves payout multipliers for each day
#[get("v1/multipliers")]
pub async fn multipliers_query(
    web::Query(query): web::Query<MultipliersQuery>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, ApiError> {
    let start = query.start_date.date().and_hms(0, 0, 0);
    let end = start + Duration::days(1);

    println!("start: {} end: {}", start, end);

    use futures::TryStreamExt;

    let (values, sum) = futures::future::try_join(
        sqlx::query!(
            "
            SELECT SUM(views) page_views, project_id
            FROM views
            WHERE views.recorded BETWEEN $1 AND $2
            GROUP BY project_id
            ORDER BY page_views DESC
            ",
            start,
            end,
        )
        .fetch_many(&**pool)
        .try_filter_map(|e| async { Ok(e.right().map(|r| (r.project_id.unwrap_or_default(), r.page_views.unwrap_or_default()))) })
        .try_collect::<HashMap<i64, i64>>(),
        sqlx::query!(
            "
            SELECT SUM(views) page_views
            FROM views
            WHERE views.recorded BETWEEN $1 AND $2
            ",
            start,
            end,
        )
        .fetch_one(&**pool)
    )
    .await?;

    Ok(HttpResponse::Ok().json(json! ({
        "sum": sum.page_views.unwrap_or_default(),
        "values": values
    })))
}
