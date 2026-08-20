use common::config::DatabaseConfig;
use sqlx::{postgres::PgPoolOptions, PgPool};

pub async fn connect(cfg: &DatabaseConfig) -> Result<PgPool, sqlx::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(cfg.max_connections)
        .min_connections(cfg.min_connections)
        .connect(&cfg.url)
        .await?;

    // Run pending migrations automatically on startup.
    sqlx::migrate!("../../migrations").run(&pool).await?;

    Ok(pool)
}
