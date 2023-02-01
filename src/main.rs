mod db;
mod models;
mod routes;
mod scheduled;
mod util;

use crate::routes::index;
use crate::routes::ingest;
use crate::routes::query;
use crate::scheduled::analytics::AnalyticsQueue;
use crate::util::env::{parse_strings_from_var, parse_var};
use actix_cors::Cors;
use actix_web::{http, web, App, HttpServer};
use log::{error, info, warn};
use std::sync::Arc;
use std::time::Duration;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    if check_env_vars() {
        error!("Some environment variables are missing!");

        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Missing required environment variables",
        ));
    }

    let sentry = sentry::init(sentry::ClientOptions {
        release: sentry::release_name!(),
        traces_sample_rate: 0.1,
        enable_profiling: true,
        profiles_sample_rate: 0.1,
        ..Default::default()
    });
    if sentry.is_enabled() {
        info!("Enabled Sentry integration");
        std::env::set_var("RUST_BACKTRACE", "1");
    }

    info!("Initializing database connection");
    let client = db::init_client().await.unwrap();

    let mut scheduler = scheduled::scheduler::Scheduler::new();

    info!("Downloading MaxMind GeoLite2 country database");
    let reader = Arc::new(scheduled::maxmind::MaxMindIndexer::new().await.unwrap());
    {
        let reader_ref = reader.clone();
        scheduler.run(Duration::from_secs(60 * 60 * 24), move || {
            let reader_ref = reader_ref.clone();

            async move {
                info!("Downloading MaxMind GeoLite2 country database");
                let result = reader_ref.index().await;
                if let Err(e) = result {
                    warn!(
                        "Downloading MaxMind GeoLite2 country database failed: {:?}",
                        e
                    );
                }
                info!("Done downloading MaxMind GeoLite2 country database");
            }
        });
    }

    let analytics_queue = Arc::new(AnalyticsQueue::new());
    {
        let client_ref = client.clone();
        let analytics_queue_ref = analytics_queue.clone();
        scheduler.run(Duration::from_secs(60 * 5), move || {
            let client_ref = client_ref.clone();
            let analytics_queue_ref = analytics_queue_ref.clone();

            async move {
                info!("Indexing analytics queue");
                let result = analytics_queue_ref.index(client_ref).await;
                if let Err(e) = result {
                    warn!("Indexing analytics queue failed: {:?}", e);
                }
                info!("Done indexing analytics queue");
            }
        });
    }

    info!("Starting Actix HTTP server!");

    HttpServer::new(move || {
        App::new()
            .wrap(
                Cors::default()
                    .allowed_origin_fn(|origin, _req_head| {
                        let allowed_origins =
                            parse_strings_from_var("CORS_ALLOWED_ORIGINS").unwrap_or_default();

                        allowed_origins.contains(&"*".to_string())
                            || allowed_origins
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
            .app_data(web::Data::new(analytics_queue.clone()))
            .app_data(web::Data::new(client.clone()))
            .app_data(web::Data::new(reader.clone()))
            .wrap(sentry_actix::Sentry::new())
            .service(index::index_get)
            .service(query::multipliers_query)
            .service(ingest::downloads_ingest)
            .service(ingest::page_view_ingest)
    })
    .bind(dotenvy::var("BIND_ADDR").unwrap())?
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

    failed |= check_var::<String>("CLICKHOUSE_URL");
    failed |= check_var::<String>("CLICKHOUSE_USER");
    failed |= check_var::<String>("CLICKHOUSE_PASSWORD");
    failed |= check_var::<String>("CLICKHOUSE_DATABASE");

    failed |= check_var::<String>("MAXMIND_LICENSE_KEY");

    failed
}
