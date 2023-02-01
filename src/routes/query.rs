use crate::routes::ApiError;
use actix_web::{get, web, HttpResponse};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;

use crate::util::guards::admin_key_guard;
use clickhouse::Row;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
pub struct MultipliersQuery {
    start_date: DateTime<Utc>,
}

/// Internal route - retrieves payout multipliers for each day
#[get("v1/multipliers", guard = "admin_key_guard")]
pub async fn multipliers_query(
    web::Query(query): web::Query<MultipliersQuery>,
    client: web::Data<clickhouse::Client>,
) -> Result<HttpResponse, ApiError> {
    let start = query.start_date.date().and_hms(0, 0, 0);
    let end = start + Duration::days(1);

    #[derive(Deserialize, Row)]
    struct ProjectMultiplier {
        pub page_views: u64,
        pub project_id: u64,
    }

    let (values, sum) = futures::future::try_join(
        client
            .query(
                r#"
            SELECT COUNT(id) page_views, project_id
            FROM views
            WHERE recorded BETWEEN ? AND ?
            GROUP BY project_id
            ORDER BY page_views DESC
            "#,
            )
            .bind(start.timestamp())
            .bind(end.timestamp())
            .fetch_all::<ProjectMultiplier>(),
        client
            .query("SELECT COUNT(id) FROM views WHERE recorded BETWEEN ? AND ?")
            .bind(start.timestamp())
            .bind(end.timestamp())
            .fetch_one::<i32>(),
    )
    .await?;

    Ok(HttpResponse::Ok().json(json! ({
        "sum": sum,
        "values": values.into_iter().map(|x| (x.project_id, x.page_views)).collect::<HashMap<u64, u64>>()
    })))
}
