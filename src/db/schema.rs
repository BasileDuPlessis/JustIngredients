//! Schema setup at startup: migrations, then the check of tables, columns and indexes.

use anyhow::{Context, Result};
use sqlx::postgres::PgPool;
use tracing::{debug, info};

use super::*;

/// Initialize the database schema using the migration system
pub async fn init_database_schema(pool: &PgPool) -> Result<()> {
    info!("Initializing database schema using migrations");

    // Apply any pending migrations
    migrations::run(pool).await?;

    info!("Database schema initialized successfully");
    Ok(())
}

/// Validate that the database schema matches what the application expects
pub async fn validate_database_schema(pool: &PgPool) -> Result<()> {
    info!("Validating database schema");

    // Check that all required tables exist
    let required_tables = vec![
        "users",
        "recipes",
        "ingredients",
        "chat_messages",
        "recipe_links",
        "usage_days",
        "app_state",
        "dialogues",
        "api_tokens",
    ];
    for table_name in required_tables {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = $1 AND table_schema = 'public')"
        )
        .bind(table_name)
        .fetch_one(pool)
        .await
        .context(format!("Failed to check if table '{}' exists", table_name))?;

        if !exists {
            return Err(anyhow::anyhow!(
                "Required table '{}' does not exist",
                table_name
            ));
        }
        debug!("✓ Table '{}' exists", table_name);
    }

    // The running actions (migration 8): `json` keeps a state as written, `\u0000` included
    validate_table_columns(
        pool,
        "dialogues",
        &[
            ("chat_id", "bigint"),
            ("state", "json"),
            ("updated_at", "timestamp with time zone"),
        ],
    )
    .await?;

    // Personal API tokens (migration 11), stored by hash
    validate_table_columns(
        pool,
        "api_tokens",
        &[
            ("id", "bigint"),
            ("user_id", "bigint"),
            ("token_hash", "bytea"),
            ("hint", "text"),
            ("scopes", "ARRAY"),
            ("created_at", "timestamp with time zone"),
            ("last_used_at", "timestamp with time zone"),
            ("revoked_at", "timestamp with time zone"),
        ],
    )
    .await?;

    // Validate users table schema
    validate_table_columns(
        pool,
        "users",
        &[
            ("id", "bigint"),
            ("telegram_id", "bigint"),
            ("language_code", "character varying"),
            ("created_at", "timestamp with time zone"),
            ("updated_at", "timestamp with time zone"),
            ("unit_view", "text"),
            ("last_seen_at", "timestamp with time zone"),
        ],
    )
    .await?;

    // Validate recipes table schema
    validate_table_columns(
        pool,
        "recipes",
        &[
            ("id", "bigint"),
            ("telegram_id", "bigint"),
            ("content", "text"),
            ("recipe_name", "character varying"),
            ("created_at", "timestamp with time zone"),
            ("content_tsv", "tsvector"),
            ("note", "text"),
        ],
    )
    .await?;

    // Validate ingredients table schema
    validate_table_columns(
        pool,
        "ingredients",
        &[
            ("id", "bigint"),
            ("user_id", "bigint"),
            ("recipe_id", "bigint"),
            ("name", "character varying"),
            ("quantity", "numeric"),
            ("unit", "character varying"),
            ("raw_text", "text"),
            ("created_at", "timestamp with time zone"),
            ("updated_at", "timestamp with time zone"),
        ],
    )
    .await?;

    // Validate indexes exist
    validate_indexes(pool, "recipes", &["recipes_content_tsv_idx"]).await?;
    validate_indexes(
        pool,
        "ingredients",
        &["ingredients_user_id_idx", "ingredients_recipe_id_idx"],
    )
    .await?;

    info!("✓ Database schema validation completed successfully");
    Ok(())
}

/// Helper function to validate that a table has the required columns with correct types
async fn validate_table_columns(
    pool: &PgPool,
    table_name: &str,
    expected_columns: &[(&str, &str)],
) -> Result<()> {
    for (column_name, expected_type) in expected_columns {
        let column_info: Option<(String, String)> = sqlx::query_as(
            "SELECT data_type, is_nullable FROM information_schema.columns
             WHERE table_name = $1 AND column_name = $2 AND table_schema = 'public'",
        )
        .bind(table_name)
        .bind(column_name)
        .fetch_optional(pool)
        .await
        .context(format!(
            "Failed to check column '{}' in table '{}'",
            column_name, table_name
        ))?;

        match column_info {
            Some((data_type, _is_nullable)) => {
                // Check if the data type matches (allowing for some flexibility)
                if !data_type
                    .to_lowercase()
                    .contains(&expected_type.to_lowercase())
                {
                    return Err(anyhow::anyhow!(
                        "Column '{}' in table '{}' has type '{}', expected '{}'",
                        column_name,
                        table_name,
                        data_type,
                        expected_type
                    ));
                }
                debug!(
                    "✓ Column '{}' in '{}' has correct type '{}'",
                    column_name, table_name, data_type
                );
            }
            None => {
                return Err(anyhow::anyhow!(
                    "Required column '{}' does not exist in table '{}'",
                    column_name,
                    table_name
                ));
            }
        }
    }
    Ok(())
}

/// Helper function to validate that required indexes exist
async fn validate_indexes(
    pool: &PgPool,
    table_name: &str,
    expected_indexes: &[&str],
) -> Result<()> {
    for index_name in expected_indexes {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE tablename = $1 AND indexname = $2)",
        )
        .bind(table_name)
        .bind(index_name)
        .fetch_one(pool)
        .await
        .context(format!(
            "Failed to check if index '{}' exists on table '{}'",
            index_name, table_name
        ))?;

        if !exists {
            return Err(anyhow::anyhow!(
                "Required index '{}' does not exist on table '{}'",
                index_name,
                table_name
            ));
        }
        debug!("✓ Index '{}' exists on table '{}'", index_name, table_name);
    }
    Ok(())
}
