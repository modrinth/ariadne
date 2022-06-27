use dashmap::DashMap;
use sqlx::PgPool;
use std::hash::Hash;

#[derive(Eq, PartialEq, Hash, Clone)]
struct NumericKey {
    project_id: String,
    site_path: String,
}

#[derive(Eq, PartialEq, Hash, Clone)]
struct RevenueKey {
    project_id: String,
}

pub struct AnalyticsQueue {
    views_queue: DashMap<NumericKey, u32>,
    downloads_queue: DashMap<NumericKey, u32>,
    revenue_queue: DashMap<RevenueKey, f32>,
}

// Batches analytics data points + transactions every few minutes
impl AnalyticsQueue {
    pub fn new() -> Self {
        AnalyticsQueue {
            views_queue: DashMap::with_capacity(1000),
            downloads_queue: DashMap::with_capacity(1000),
            revenue_queue: DashMap::with_capacity(1000),
        }
    }

    pub async fn add_view(&self, project_id: String, site_path: String) {
        let key = NumericKey {
            project_id,
            site_path,
        };

        if let Some(mut val) = self.views_queue.get_mut(&key) {
            *val += 1;
        } else {
            self.views_queue.insert(key, 1);
        }
    }

    pub async fn add_download(&self, project_id: String, site_path: String) {
        let key = NumericKey {
            project_id,
            site_path,
        };

        if let Some(mut val) = self.downloads_queue.get_mut(&key) {
            *val += 1;
        } else {
            self.downloads_queue.insert(key, 1);
        }
    }

    pub async fn add_revenue(&self, project_id: String, revenue: f32) {
        let key = RevenueKey { project_id };

        if let Some(mut val) = self.revenue_queue.get_mut(&key) {
            *val += revenue;
        } else {
            self.revenue_queue.insert(key, revenue);
        }
    }

    pub async fn index(&self, pool: &PgPool) -> Result<(), sqlx::Error> {
        //TODO: This double allocates all of the queues. Could be avoided, not sure how.
        let views_queue = self.views_queue.clone();
        self.downloads_queue.clear();

        let downloads_queue = self.downloads_queue.clone();
        self.views_queue.clear();

        let revenue_queue = self.revenue_queue.clone();
        self.revenue_queue.clear();

        if !views_queue.is_empty() || !downloads_queue.is_empty() || !revenue_queue.is_empty() {
            let mut transaction = pool.begin().await?;

            for (key, value) in views_queue {
                sqlx::query!(
                    "
                    INSERT INTO views (views, project_id, site_path)
                    VALUES ($1, $2, $3)
                    ",
                    value as i32,
                    key.project_id,
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
                    value as u32,
                    key.project_id,
                    key.site_path,
                )
                .execute(&mut *transaction)
                .await?;
            }

            for (key, value) in revenue_queue {
                sqlx::query!(
                    "
                    INSERT INTO revenue (money, project_id)
                    VALUES ($1, $2)
                    ",
                    value,
                    key.project_id,
                )
                .execute(&mut *transaction)
                .await?;
            }

            transaction.commit().await?;
        }

        Ok(())
    }
}
