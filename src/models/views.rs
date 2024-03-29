use clickhouse::Row;
use serde::Serialize;
use std::hash::{Hash, Hasher};
use std::net::Ipv6Addr;
use uuid::Uuid;

#[derive(Row, Serialize, Clone)]
pub struct PageView {
    #[serde(with = "uuid::serde::compact")]
    pub id: Uuid,
    pub recorded: i64,
    pub domain: String,
    pub site_path: String,
    pub from_server: bool,

    // Modrinth User ID for logged in users (unused atm)
    pub user_id: u64,
    // Modrinth Project ID (used for payouts)
    pub project_id: u64,

    // The below information is used exclusively for data aggregation and fraud detection
    // (ex: page view botting).
    pub ip: Ipv6Addr,
    pub country: String,
    pub user_agent: String,
    pub headers: Vec<(String, String)>,
}

impl PartialEq<Self> for PageView {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for PageView {}

impl Hash for PageView {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}
