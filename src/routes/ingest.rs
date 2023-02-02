use crate::models::downloads::Download;
use crate::models::views::PageView;
use crate::routes::ApiError;
use crate::scheduled::maxmind::MaxMindIndexer;
use crate::util::base62::parse_base62;
use crate::util::env::parse_strings_from_var;
use crate::util::guards::admin_key_guard;
use crate::AnalyticsQueue;
use actix_web::{post, web};
use actix_web::{HttpRequest, HttpResponse};
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;
use std::net::{AddrParseError, IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use url::Url;
use uuid::Uuid;

const FILTERED_HEADERS: &[&str] = &[
    "authorization",
    "cookie",
    "modrinth-admin",
    // we already retrieve/use these elsewhere- so they are unneeded
    "user-agent",
    "cf-connecting-ip",
    "cf-ipcountry",
    "x-forwarded-for",
    "x-real-ip",
    // We don't need the information vercel provides from its headers
    "x-vercel-ip-city",
    "x-vercel-ip-timezone",
    "x-vercel-ip-longitude",
    "x-vercel-proxy-signature",
    "x-vercel-ip-country-region",
    "x-vercel-forwarded-for",
    "x-vercel-proxied-for",
    "x-vercel-proxy-signature-ts",
    "x-vercel-ip-latitude",
    "x-vercel-ip-country",
];

fn convert_to_ip_v6(src: &str) -> Result<Ipv6Addr, AddrParseError> {
    let ip_addr: IpAddr = src.parse()?;

    Ok(match ip_addr {
        IpAddr::V4(x) => x.to_ipv6_mapped(),
        IpAddr::V6(x) => x,
    })
}

#[derive(Deserialize)]
pub struct DownloadInput {
    ip: String,
    url: String,
    project_id: String,
    version_id: String,
    headers: HashMap<String, String>,
}

// Internal (can only be called with key) - protections are lax
// called from labrinth- URLs guaranteed to be valid
#[post("v1/download", guard = "admin_key_guard")]
pub async fn downloads_ingest(
    maxmind: web::Data<Arc<MaxMindIndexer>>,
    analytics_queue: web::Data<Arc<AnalyticsQueue>>,
    url_input: web::Json<DownloadInput>,
) -> Result<HttpResponse, ApiError> {
    let url = Url::parse(&url_input.url)
        .map_err(|_| ApiError::InvalidInput("invalid download URL specified!".to_string()))?;

    let parsed_pid = parse_base62(&url_input.project_id)
        .map_err(|_| ApiError::InvalidInput("invalid project ID in download URL!".to_string()))?;
    let parsed_vid = parse_base62(&url_input.version_id)
        .map_err(|_| ApiError::InvalidInput("invalid version ID in download URL!".to_string()))?;

    let ip = convert_to_ip_v6(&url_input.ip)
        .unwrap_or_else(|_| Ipv4Addr::new(127, 0, 0, 1).to_ipv6_mapped());

    analytics_queue
        .add_download(Download {
            id: Uuid::new_v4(),
            recorded: Utc::now().timestamp_nanos() / 100_000,
            domain: url.host_str().unwrap_or_default().to_string(),
            site_path: url.path().to_string(),
            user_id: 0,
            project_id: parsed_pid,
            version_id: parsed_vid,
            ip,
            country: maxmind.query(ip).await.unwrap_or_default(),
            user_agent: url_input
                .headers
                .get("user-agent")
                .cloned()
                .unwrap_or_default(),
            headers: url_input
                .headers
                .clone()
                .into_iter()
                .filter(|x| !FILTERED_HEADERS.contains(&&*x.0.to_lowercase()))
                .collect(),
        })
        .await;

    Ok(HttpResponse::NoContent().body(""))
}

#[derive(Deserialize)]
pub struct UrlInput {
    url: String,

    // These will only be sent from the Nuxt.JS server
    ip: Option<String>,
    headers: Option<HashMap<String, String>>,
}

//this route should be behind the cloudflare WAF to prevent non-browsers from calling it
#[post("v1/view")]
pub async fn page_view_ingest(
    req: HttpRequest,
    maxmind: web::Data<Arc<MaxMindIndexer>>,
    analytics_queue: web::Data<Arc<AnalyticsQueue>>,
    url_input: web::Json<UrlInput>,
) -> Result<HttpResponse, ApiError> {
    let admin_key = dotenvy::var("ARIADNE_ADMIN_KEY")?;

    let conn_info = req.connection_info().peer_addr().map(|x| x.to_string());

    let url = Url::parse(&url_input.url)
        .map_err(|_| ApiError::InvalidInput("invalid page view URL specified!".to_string()))?;

    let domain = url
        .host_str()
        .ok_or_else(|| ApiError::InvalidInput("invalid page view URL specified!".to_string()))?;

    let allowed_origins = parse_strings_from_var("CORS_ALLOWED_ORIGINS").unwrap_or_default();
    if !(domain.ends_with(".modrinth.com")
        || domain == "modrinth.com"
        || allowed_origins.contains(&"*".to_string()))
    {
        return Err(ApiError::InvalidInput(
            "invalid page view URL specified!".to_string(),
        ));
    }

    let from_server = req
        .headers()
        .get(crate::util::guards::ADMIN_KEY_HEADER)
        .map(|x| x.to_str().unwrap_or_default() == &*admin_key)
        .unwrap_or(false);

    let temp_headers = req
        .headers()
        .into_iter()
        .map(|(key, val)| {
            (
                key.to_string().to_lowercase(),
                val.to_str().unwrap_or_default().to_string(),
            )
        })
        .collect::<HashMap<String, String>>();

    let headers = if from_server {
        if let Some(headers) = &url_input.headers {
            headers.clone()
        } else {
            temp_headers
        }
    } else {
        temp_headers
    };

    let ip = convert_to_ip_v6(if from_server && url_input.ip.is_some() {
        url_input.ip.as_deref().unwrap()
    } else if let Some(header) = headers.get("cf-connecting-ip") {
        header
    } else {
        conn_info.as_deref().unwrap_or_default()
    })
    .unwrap_or_else(|_| Ipv4Addr::new(127, 0, 0, 1).to_ipv6_mapped());

    let mut view = PageView {
        id: Uuid::new_v4(),
        recorded: Utc::now().timestamp_nanos() / 100_000,
        domain: domain.to_string(),
        site_path: url.path().to_string(),
        from_server,
        user_id: 0,
        project_id: 0,
        ip,
        country: maxmind.query(ip).await.unwrap_or_default(),
        user_agent: headers.get("user-agent").cloned().unwrap_or_default(),
        headers: headers.into_iter().filter(|x| !FILTERED_HEADERS.contains(&&*x.0)).collect(),
    };

    if let Some(segments) = url.path_segments() {
        let segments_vec = segments.collect::<Vec<_>>();

        if segments_vec.len() >= 2 {
            //todo: fetch from labrinth periodically when route exists
            const PROJECT_TYPES: &[&str] = &[
                "mod",
                "modpack",
                "plugin",
                "resourcepack",
                "shader",
                "datapack",
            ];

            if PROJECT_TYPES.contains(&segments_vec[0]) {
                #[derive(Deserialize)]
                struct CheckResponse {
                    id: String,
                }

                let client = reqwest::Client::new();

                let response = client
                    .get(format!(
                        "{}project/{}/check",
                        dotenvy::var("LABRINTH_API_URL")?,
                        &segments_vec[1]
                    ))
                    .header("x-ratelimit-key", dotenvy::var("LABRINTH_RATE_LIMIT_KEY")?)
                    .send()
                    .await?;

                if response.status().is_success() {
                    let check_response = response.json::<CheckResponse>().await?;

                    view.project_id = parse_base62(&check_response.id).unwrap_or_default();
                }
            }
        }
    }

    analytics_queue.add_view(view).await;

    Ok(HttpResponse::NoContent().body(""))
}
