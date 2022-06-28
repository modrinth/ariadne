mod base62;
mod guards;
mod routes;
mod scheduled;
mod util;

use crate::routes::index;
use crate::routes::ingest;
use crate::scheduled::analytics::AnalyticsQueue;
use crate::scheduled::ratelimit::RateLimitQueue;
use crate::util::{parse_strings_from_var, parse_var};
use actix_cors::Cors;
use actix_web::{http, web, App, HttpServer};
use log::{error, info, warn};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use std::time::Duration;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    if check_env_vars() {
        error!("Some environment variables are missing!");

        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Missing required environment variables",
        ));
    }

    info!("Initializing database connection");
    let database_url = dotenv::var("DATABASE_URL").expect("`DATABASE_URL` not in .env");

    let pool = PgPoolOptions::new()
        .min_connections(
            dotenv::var("DATABASE_MIN_CONNECTIONS")
                .ok()
                .and_then(|x| x.parse().ok())
                .unwrap_or(0),
        )
        .max_connections(
            dotenv::var("DATABASE_MAX_CONNECTIONS")
                .ok()
                .and_then(|x| x.parse().ok())
                .unwrap_or(16),
        )
        .max_lifetime(Some(Duration::from_secs(60 * 60)))
        .connect(&database_url)
        .await
        .expect("Database connection failed");

    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("Error while applying database migrations.");

    let mut scheduler = scheduled::scheduler::Scheduler::new();

    let analytics_queue = Arc::new(AnalyticsQueue::new());

    {
        let pool_ref = pool.clone();
        let analytics_queue_ref = analytics_queue.clone();
        scheduler.run(Duration::from_secs(60 * 5), move || {
            let pool_ref = pool_ref.clone();
            let analytics_queue_ref = analytics_queue_ref.clone();

            async move {
                info!("Indexing analytics queue");
                let result = analytics_queue_ref.index(&pool_ref).await;
                if let Err(e) = result {
                    warn!("Indexing analytics queue failed: {:?}", e);
                }
                info!("Done indexing analytics queue");
            }
        });
    }

    let rate_limit_queue = Arc::new(RateLimitQueue::new(
        dotenv::var("PEPPER").expect("Pepper not supplied in env variables!"),
    ));

    {
        let rate_limit_queue_ref = rate_limit_queue.clone();

        scheduler.run(Duration::from_secs(60 * 60), move || {
            let rate_limit_queue_ref = rate_limit_queue_ref.clone();

            async move {
                info!("Indexing rate limit queue");
                rate_limit_queue_ref.index().await;
                info!("Done indexing rate limit queue");
            }
        });
    }

    info!("Starting Actix HTTP server!");

    HttpServer::new(move || {
        App::new()
            .wrap(
                Cors::default()
                    .allowed_origin_fn(|origin, _req_head| {
                        parse_strings_from_var("ALLOWED_CALLBACK_URLS")
                            .unwrap_or_default()
                            .contains(&origin.to_str().unwrap_or_default().to_string())
                    })
                    .allowed_methods(vec!["GET", "POST"])
                    .allowed_headers(vec![
                        http::header::AUTHORIZATION,
                        http::header::ACCEPT,
                        http::header::CONTENT_TYPE,
                    ])
                    .max_age(3600),
            )
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(analytics_queue.clone()))
            .app_data(web::Data::new(rate_limit_queue.clone()))
            .service(index::index_get)
            .service(ingest::revenue_ingest)
            .service(ingest::downloads_ingest)
    })
    .bind(dotenv::var("BIND_ADDR").unwrap())?
    .run()
    .await
}

// This is so that env vars not used immediately don't panic at runtime
fn check_env_vars() -> bool {
    let mut failed = false;

    fn check_var<T: std::str::FromStr>(var: &'static str) -> bool {
        let check = parse_var::<T>(var).is_none();
        if check {
            warn!(
                "Variable `{}` missing in dotenv or not of type `{}`",
                var,
                std::any::type_name::<T>()
            );
        }
        check
    }

    if parse_strings_from_var("CORS_ALLOWED_ORIGINS").is_none() {
        warn!("Variable `CORS_ALLOWED_ORIGINS` missing in dotenv or not a json array of strings");
        failed |= true;
    }

    failed |= check_var::<String>("BIND_ADDR");

    failed |= check_var::<String>("DATABASE_URL");

    failed |= check_var::<String>("ARIADNE_ADMIN_KEY");

    failed |= check_var::<String>("PEPPER");

    failed |= check_var::<String>("LABRINTH_API_URL");
    failed |= check_var::<String>("LABRINTH_RATE_LIMIT_KEY");

    failed
}
