use dashmap::DashMap;
use sha2::Digest;
use std::net::{IpAddr, Ipv4Addr};

#[derive(Eq, PartialEq, Hash, Clone)]
struct PageViewEntry {
    // hashed ip + pepper
    ip: String,
    site_path: String,
}

// limits page views to 5 recorded every hour per IP
pub struct RateLimitQueue {
    pepper: String,
    views_queue: DashMap<PageViewEntry, u32>,
}

impl RateLimitQueue {
    pub fn new(pepper: String) -> Self {
        RateLimitQueue {
            pepper,
            views_queue: DashMap::with_capacity(1000),
        }
    }

    pub async fn add(&self, ip: String, site_path: String) -> bool {
        let ip_addr: IpAddr = ip
            .parse()
            .unwrap_or(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));

        let ip = match ip_addr {
            IpAddr::V4(x) => x.to_string(),
            IpAddr::V6(x) => format!("{:X?}", &x.segments()[0..4]),
        };

        let mut hasher = sha2::Sha256::new();
        hasher.update(format!("{}{}", ip, self.pepper));
        let result = &hasher.finalize()[..];

        let key = PageViewEntry {
            ip: format!("{:X?}", result),
            site_path,
        };

        if let Some(mut val) = self.views_queue.get_mut(&key) {
            *val += 1;

            if val.value() >= &5 {
                return false;
            }
        } else {
            self.views_queue.insert(key, 0);
        }

        true
    }

    pub async fn index(&self) {
        self.views_queue.clear();
    }
}
