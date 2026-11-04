//! Versioned schema migrations (backlog IMP-028): the SQL files of `migrations/`
//! (`NNNN_name.up.sql`, with its `.down.sql`), embedded at build time and applied at startup by
//! sqlx, each in a transaction, under an advisory lock, recorded with its checksum in
//! `_sqlx_migrations`. A new schema change is a new pair of files.
//!
//! Databases created before the switch recorded their versions in `schema_migrations`: the first
//! start marks those versions applied in `_sqlx_migrations` without running them again
//! (`adopt_legacy_versions`). `schema_migrations` is kept as it is, so that a release from before
//! the switch still reads its version after a rollback.

use anyhow::{Context, Result};
use sqlx::migrate::{Migrate, Migrator};
use sqlx::postgres::PgPool;
use tracing::info;

/// The migrations of `migrations/`, embedded at build time (`build.rs` rebuilds on a change).
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Applies the pending migrations, after adopting the versions recorded by the former
/// migration table.
pub async fn run(pool: &PgPool) -> Result<()> {
    adopt_legacy_versions(pool).await?;
    MIGRATOR
        .run(pool)
        .await
        .context("Failed to apply the migrations")?;
    info!(
        "Database schema is at version {}",
        current_version(pool).await?
    );
    Ok(())
}

/// Reverts the migrations above `target` with their down scripts, newest first.
pub async fn undo_to(pool: &PgPool, target: i64) -> Result<()> {
    MIGRATOR
        .undo(pool, target)
        .await
        .with_context(|| format!("Failed to revert the migrations above {target}"))
}

/// The highest version applied, 0 for a database without migrations.
pub async fn current_version(pool: &PgPool) -> Result<i64> {
    if !table_exists(pool, "_sqlx_migrations").await? {
        return Ok(0);
    }
    let version: Option<i64> =
        sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations WHERE success")
            .fetch_one(pool)
            .await
            .context("Failed to read the schema version")?;
    Ok(version.unwrap_or(0))
}

/// A database migrated by the former system (`schema_migrations`, up to version N) and not yet
/// by sqlx: versions 1 to N are recorded in `_sqlx_migrations` as applied, with the checksums of
/// the embedded files, so that sqlx runs only what comes after. In one transaction; does nothing
/// once `_sqlx_migrations` exists.
async fn adopt_legacy_versions(pool: &PgPool) -> Result<()> {
    if table_exists(pool, "_sqlx_migrations").await?
        || !table_exists(pool, "schema_migrations").await?
    {
        return Ok(());
    }
    let legacy: Option<i32> = sqlx::query_scalar("SELECT MAX(version) FROM schema_migrations")
        .fetch_one(pool)
        .await
        .context("Failed to read schema_migrations")?;
    let legacy = i64::from(legacy.unwrap_or(0));
    if legacy == 0 {
        return Ok(());
    }

    let mut tx = pool.begin().await?;
    tx.ensure_migrations_table()
        .await
        .context("Failed to create _sqlx_migrations")?;
    let mut adopted = 0;
    for migration in MIGRATOR
        .iter()
        .filter(|migration| migration.migration_type.is_up_migration())
        .filter(|migration| migration.version <= legacy)
    {
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) \
             VALUES ($1, $2, TRUE, $3, 0)",
        )
        .bind(migration.version)
        .bind(&*migration.description)
        .bind(&*migration.checksum)
        .execute(&mut *tx)
        .await
        .with_context(|| format!("Failed to adopt migration {}", migration.version))?;
        adopted += 1;
    }
    tx.commit().await?;
    info!(
        versions = adopted,
        "Adopted the versions of schema_migrations: not run again"
    );
    Ok(())
}

async fn table_exists(pool: &PgPool, table: &str) -> Result<bool> {
    sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
        .bind(table)
        .fetch_one(pool)
        .await
        .with_context(|| format!("Failed to check whether {table} exists"))
}
