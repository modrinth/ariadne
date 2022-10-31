use dashmap::DashMap;
use sqlx::PgPool;
use std::hash::Hash;

#[derive(Eq, PartialEq, Hash, Clone)]
struct DownloadKey {
    project_id: u64,
    site_path: String,
}

#[derive(Eq, PartialEq, Hash, Clone)]
struct PageViewKey {
    project_id: Option<u64>,
    site_path: String,
}

pub struct AnalyticsQueue {
    views_queue: DashMap<PageViewKey, u32>,
    downloads_queue: DashMap<DownloadKey, u32>,
}

// Batches analytics data points + transactions every few minutes
impl AnalyticsQueue {
    pub fn new() -> Self {
        AnalyticsQueue {
            views_queue: DashMap::with_capacity(1000),
            downloads_queue: DashMap::with_capacity(1000),
        }
    }

    pub async fn add_view(&self, project_id: Option<u64>, site_path: String) {
        let key = PageViewKey {
            project_id,
            site_path,
        };

        if let Some(mut val) = self.views_queue.get_mut(&key) {
            *val += 1;
        } else {
            self.views_queue.insert(key, 1);
        }
    }

    pub async fn add_download(&self, project_id: u64, site_path: String) {
        let key = DownloadKey {
            project_id,
            site_path,
        };

        if let Some(mut val) = self.downloads_queue.get_mut(&key) {
            *val += 1;
        } else {
            self.downloads_queue.insert(key, 1);
        }
    }

    pub async fn index(&self, pool: &PgPool) -> Result<(), sqlx::Error> {
        //TODO: This double allocates all of the queues. Could be avoided, not sure how.
        let views_queue = self.views_queue.clone();
        self.views_queue.clear();

        let downloads_queue = self.downloads_queue.clone();
        self.downloads_queue.clear();

        if !views_queue.is_empty() || !downloads_queue.is_empty() {
            let mut transaction = pool.begin().await?;

            for (key, value) in views_queue {
                sqlx::query!(
                    "
                    INSERT INTO views (views, project_id, site_path)
                    VALUES ($1, $2, $3)
                    ",
                    value as i32,
                    key.project_id.map(|x| x as i64),
                    key.site_path,
                )
                .execute(&mut *transaction)
                .await?;
            }

            for (key, value) in downloads_queue {
                sqlx::query!(
                    "
                    INSERT INTO downloads (downloads, project_id, site_path)
                    VALUES ($1, $2, $3)
                    ",
                    value as i32,
                    key.project_id as i64,
                    key.site_path,
                )
                .execute(&mut *transaction)
                .await?;
            }

            transaction.commit().await?;
        }

        Ok(())
    }
}
