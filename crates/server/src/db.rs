use sqlx::PgPool;
use tracing::{info, warn};

/// Create a PostgreSQL connection pool and run migrations.
/// Returns None if PG_URL is not configured.
pub async fn init_pg_pool(config: &stupid_core::config::PostgresConfig) -> Option<PgPool> {
    // PG_URL must be explicitly set — don't try to connect with only the
    // default constructed connection string (host/port fields).
    if config.pg_url.is_none() {
        info!("PG_URL not set — embedding/vector features disabled");
        return None;
    }
    let url = config.database_url();
    // Log URL with password masked for debugging connection issues
    let masked = if let Some(at_pos) = url.find('@') {
        let scheme_end = url.find("://").map(|p| p + 3).unwrap_or(0);
        format!("{}***@{}", &url[..scheme_end], &url[at_pos + 1..])
    } else {
        url.clone()
    };
    info!("Connecting to PostgreSQL: {}", masked);

    match PgPool::connect(&url).await {
        Ok(pool) => {
            info!("PostgreSQL connected via PG_URL");
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
