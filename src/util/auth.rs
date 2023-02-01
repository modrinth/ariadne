use crate::routes::ApiError;
use actix_web::http::header::HeaderMap;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub role: Role,
}

#[derive(Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Developer,
    Moderator,
    Admin,
}

#[derive(Deserialize)]
pub struct Team {
    pub id: String,
    pub members: Vec<TeamMember>,
}

#[derive(Deserialize)]
pub struct TeamMember {
    pub team_id: String,
    pub user: User,
    pub role: String,
    pub permissions: u32,
    pub accepted: bool,
}

pub async fn check_is_authorized(
    project_id: Option<&str>,
    headers: &HeaderMap,
    use_payouts_permission: bool,
) -> Result<(), ApiError> {
    let token = headers
        .get("Authorization")
        .ok_or_else(|| ApiError::Authentication("missing 'Authorization' header".to_string()))?
        .to_str()
        .map_err(|_| ApiError::Authentication("invalid 'Authorization' header".to_string()))?;

    let client = reqwest::Client::new();

    let user: User = client
        .get(format!("{}user", dotenvy::var("LABRINTH_API_URL")?))
        .header("x-ratelimit-key", dotenvy::var("LABRINTH_RATE_LIMIT_KEY")?)
        .header("Authorization", token)
        .send()
        .await?
        .json()
        .await?;

    if user.role != Role::Admin {
        if let Some(project_id) = project_id {
            let members: Team = client
                .get(format!(
                    "{}project/{}/members",
                    dotenvy::var("LABRINTH_API_URL")?,
                    project_id
                ))
                .header("x-ratelimit-key", dotenvy::var("LABRINTH_RATE_LIMIT_KEY")?)
                .header("Authorization", token)
                .send()
                .await?
                .json()
                .await?;

            const VIEW_ANALYTICS: u32 = 1 << 8;
            const VIEW_PAYOUTS: u32 = 1 << 9;

            let permission = if use_payouts_permission {
                VIEW_PAYOUTS
            } else {
                VIEW_ANALYTICS
            };

            members
                .members
                .iter()
                .find(|x| {
                    x.user.id == user.id && x.accepted && (x.permissions & permission) == permission
                })
                .ok_or_else(|| {
                    ApiError::Authentication(
                        "You are not allowed to view analytics from this team!".to_string(),
                    )
                })?;
        } else {
            return Err(ApiError::Authentication(
                "Please specify a project ID".to_string(),
            ));
        }
    }

    Ok(())
}
