use crate::config::AppConfig;
use anyhow::Result;
use sqlx::PgPool;
use std::time::Duration;
use tracing::{error, info};

const REINDEX_ADVISORY_LOCK_KEY: i64 = 7_311_201_001;

/// Reindex service that runs REINDEX CONCURRENTLY on transactions table indexes
/// every XX hours to prevent index bloat
pub struct TransactionReindexService {
    pool: PgPool,
    interval_hours: u64,
}

impl TransactionReindexService {
    pub fn new(pool: PgPool, interval_hours: u64) -> Self {
        Self {
            pool,
            interval_hours,
        }
    }

    /// Start the reindex service loop
    pub async fn run(self) {
        info!(
            "Transaction Reindex Service started (interval: {} hours)",
            self.interval_hours
        );

        // Run first reindex immediately on startup
        self.run_reindex_cycle().await;

        loop {
            // Wait for the configured interval
            let duration = Duration::from_secs(self.interval_hours * 3600);
            info!(
                "Waiting {} hours until next reindex operation...",
                self.interval_hours
            );
            tokio::time::sleep(duration).await;

            self.run_reindex_cycle().await;
        }
    }

    /// Run a complete reindex cycle for both indexes
    async fn run_reindex_cycle(&self) {
        use std::time::Instant;

        let mut conn = match self.pool.acquire().await {
            Ok(conn) => conn,
            Err(e) => {
                error!("Failed to acquire DB connection for reindex cycle: {}", e);
                return;
            }
        };

        let lock_acquired = match sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_lock($1)")
            .bind(REINDEX_ADVISORY_LOCK_KEY)
            .fetch_one(&mut *conn)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                error!("Failed to acquire reindex advisory lock: {}", e);
                return;
            }
        };

        if !lock_acquired {
            info!("Skipping reindex cycle: another worker is already reindexing.");
            return;
        }

        info!("Starting reindex operation for transactions table indexes");
        let cycle_start = Instant::now();

        // Reindex transactions_pkey
        match self.reindex_transactions_pkey(&mut conn).await {
            Ok(_) => info!("Successfully reindexed transactions_pkey"),
            Err(e) => error!("Failed to reindex transactions_pkey: {}", e),
        }

        // Reindex transactions_block_time_idx
        match self.reindex_transactions_block_time_idx(&mut conn).await {
            Ok(_) => info!("Successfully reindexed transactions_block_time_idx"),
            Err(e) => error!("Failed to reindex transactions_block_time_idx: {}", e),
        }

        let cycle_duration = cycle_start.elapsed();
        info!(
            "Reindex operation completed - Total time: {:.2} seconds ({:.2} minutes)",
            cycle_duration.as_secs_f64(),
            cycle_duration.as_secs_f64() / 60.0
        );

        if let Err(e) = sqlx::query("SELECT pg_advisory_unlock($1)")
            .bind(REINDEX_ADVISORY_LOCK_KEY)
            .execute(&mut *conn)
            .await
        {
            error!("Failed to release reindex advisory lock: {}", e);
        }
    }

    /// Reindex the primary key index on transactions table
    /// Uses REINDEX CONCURRENTLY to avoid blocking reads/writes
    async fn reindex_transactions_pkey(
        &self,
        conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    ) -> Result<()> {
        use std::time::Instant;

        info!("Starting REINDEX on transactions_pkey...");
        let start = Instant::now();

        // REINDEX CONCURRENTLY allows reads and writes to continue during reindex
        sqlx::query("REINDEX INDEX CONCURRENTLY transactions_pkey")
            .execute(&mut **conn)
            .await?;

        let duration = start.elapsed();
        info!(
            "Completed REINDEX on transactions_pkey - Time: {:.2} seconds ({:.2} minutes)",
            duration.as_secs_f64(),
            duration.as_secs_f64() / 60.0
        );

        Ok(())
    }

    /// Reindex the block_time index on transactions table
    /// Uses REINDEX CONCURRENTLY to avoid blocking reads/writes
    async fn reindex_transactions_block_time_idx(
        &self,
        conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    ) -> Result<()> {
        use std::time::Instant;

        info!("Starting REINDEX on transactions_block_time_idx...");
        let start = Instant::now();

        // REINDEX CONCURRENTLY allows reads and writes to continue during reindex
        sqlx::query("REINDEX INDEX CONCURRENTLY transactions_block_time_idx")
            .execute(&mut **conn)
            .await?;

        let duration = start.elapsed();
        info!(
            "Completed REINDEX on transactions_block_time_idx - Time: {:.2} seconds ({:.2} minutes)",
            duration.as_secs_f64(),
            duration.as_secs_f64() / 60.0
        );

        Ok(())
    }
}

/// Start the reindex service as a background task
pub async fn start_reindex_service(_config: AppConfig, pool: PgPool) {
    // Default to 12 hours interval
    let interval_hours = 12;

    let service = TransactionReindexService::new(pool, interval_hours);

    // Run the service (this will loop forever)
    service.run().await;
}
