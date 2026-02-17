use sqlx::PgPool;
use tracing::{info, warn};

/// Create a PostgreSQL connection pool and run migrations.
/// Returns None if PG_URL is not configured.
pub async fn init_pg_pool(config: &stupid_core::config::PostgresConfig) -> Option<PgPool> {
    let url = config.database_url();
    if url.is_empty() || url == "postgres://:@localhost:5432/stupiddb?sslmode=prefer" {
        warn!("PG_URL not configured — embedding/vector features disabled");
        return None;
    }

    match PgPool::connect(&url).await {
        Ok(pool) => {
            info!("PostgreSQL connected: {}", config.host);
            match sqlx::migrate!("../../migrations").run(&pool).await {
                Ok(_) => {
                    info!("Database migrations applied successfully");
                    Some(pool)
                }
                Err(e) => {
                    warn!("Failed to run migrations: {} — embedding features disabled", e);
                    None
                }
            }
        }
        Err(e) => {
            warn!("Failed to connect to PostgreSQL: {} — embedding features disabled", e);
            None
        }
    }
}
