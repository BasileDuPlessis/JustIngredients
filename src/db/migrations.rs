//! Versioned schema migrations, applied at startup, with down scripts.

use anyhow::{Context, Result};
use sqlx::postgres::PgPool;
use tracing::info;

/// Represents a database migration
pub struct Migration {
    pub version: i32,
    pub name: &'static str,
    pub up: &'static str,
    pub down: Option<&'static str>,
}

/// Get all available migrations in order
pub fn get_migrations() -> Vec<Migration> {
    vec![
        Migration {
            version: 1,
            name: "create_initial_tables",
            up: r#"
                -- Create users table
                CREATE TABLE IF NOT EXISTS users (
                    id BIGSERIAL PRIMARY KEY,
                    telegram_id BIGINT UNIQUE NOT NULL,
                    language_code VARCHAR(10) DEFAULT 'en',
                    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
                    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
                );

                -- Create recipes table
                CREATE TABLE IF NOT EXISTS recipes (
                    id BIGSERIAL PRIMARY KEY,
                    telegram_id BIGINT NOT NULL,
                    content TEXT NOT NULL,
                    recipe_name VARCHAR(255),
                    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
                    content_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english', content)) STORED
                );

                -- Create ingredients table
                CREATE TABLE IF NOT EXISTS ingredients (
                    id BIGSERIAL PRIMARY KEY,
                    user_id BIGINT NOT NULL REFERENCES users(id),
                    recipe_id BIGINT REFERENCES recipes(id),
                    name VARCHAR(255) NOT NULL,
                    quantity DECIMAL(10,3),
                    unit VARCHAR(50),
                    raw_text TEXT,
                    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
                    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
                );

                -- Create indexes
                CREATE INDEX IF NOT EXISTS recipes_content_tsv_idx ON recipes USING GIN (content_tsv);
                CREATE INDEX IF NOT EXISTS ingredients_user_id_idx ON ingredients(user_id);
                CREATE INDEX IF NOT EXISTS ingredients_recipe_id_idx ON ingredients(recipe_id);
            "#,
            down: Some(
                r#"
                DROP TABLE IF EXISTS ingredients;
                DROP TABLE IF EXISTS recipes;
                DROP TABLE IF EXISTS users;
            "#,
            ),
        },
        Migration {
            version: 2,
            name: "recipes_owner_index_and_ingredient_cascade",
            // Every user-facing query filters recipes by owner, newest first; deleting a recipe
            // deletes its ingredients.
            up: r#"
                CREATE INDEX IF NOT EXISTS recipes_telegram_id_created_at_idx
                    ON recipes (telegram_id, created_at DESC, id DESC);
                ALTER TABLE ingredients DROP CONSTRAINT IF EXISTS ingredients_recipe_id_fkey;
                ALTER TABLE ingredients ADD CONSTRAINT ingredients_recipe_id_fkey
                    FOREIGN KEY (recipe_id) REFERENCES recipes(id) ON DELETE CASCADE;
            "#,
            down: Some(
                r#"
                ALTER TABLE ingredients DROP CONSTRAINT IF EXISTS ingredients_recipe_id_fkey;
                ALTER TABLE ingredients ADD CONSTRAINT ingredients_recipe_id_fkey
                    FOREIGN KEY (recipe_id) REFERENCES recipes(id);
                DROP INDEX IF EXISTS recipes_telegram_id_created_at_idx;
            "#,
            ),
        },
        Migration {
            version: 3,
            name: "chat_messages",
            // Ids of the bot's and the user's messages that a later action deletes or edits
            // (UX plan U5): never their content
            up: r#"
                CREATE TABLE IF NOT EXISTS chat_messages (
                    chat_id BIGINT NOT NULL,
                    message_id BIGINT NOT NULL,
                    kind TEXT NOT NULL,
                    recipe_id BIGINT,
                    sent_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    PRIMARY KEY (chat_id, message_id)
                );
                CREATE INDEX IF NOT EXISTS chat_messages_chat_kind_idx
                    ON chat_messages (chat_id, kind);
            "#,
            down: Some(
                r#"
                DROP TABLE IF EXISTS chat_messages;
            "#,
            ),
        },
        Migration {
            version: 4,
            name: "number_duplicate_recipe_names",
            // The numbering itself runs at every startup in Rust
            // (`number_duplicate_recipe_names`, the rule of new names, idempotent): the first
            // SQL version of this migration (applied on staging) could leave duplicates
            up: r#"
                SELECT 1;
            "#,
            down: Some(
                r#"
                SELECT 1;
            "#,
            ),
        },
        Migration {
            version: 5,
            name: "users_unit_view",
            // How the user's recipe cards show measures (UX plan U8): `unit_view::UnitView`
            up: r#"
                ALTER TABLE users ADD COLUMN IF NOT EXISTS unit_view TEXT NOT NULL DEFAULT 'french';
            "#,
            down: Some(
                r#"
                ALTER TABLE users DROP COLUMN IF EXISTS unit_view;
            "#,
            ),
        },
        Migration {
            version: 6,
            name: "recipe_links",
            // Recipes linked together, both ways (UX plan U9): one row per pair, the smaller
            // id first, gone with either recipe
            up: r#"
                CREATE TABLE IF NOT EXISTS recipe_links (
                    recipe_a BIGINT NOT NULL REFERENCES recipes(id) ON DELETE CASCADE,
                    recipe_b BIGINT NOT NULL REFERENCES recipes(id) ON DELETE CASCADE,
                    PRIMARY KEY (recipe_a, recipe_b),
                    CHECK (recipe_a < recipe_b)
                );
                CREATE INDEX IF NOT EXISTS recipe_links_recipe_b_idx ON recipe_links (recipe_b);
            "#,
            down: Some(
                r#"
                DROP TABLE IF EXISTS recipe_links;
            "#,
            ),
        },
        Migration {
            version: 7,
            name: "usage_history",
            // Plan O3: each user's last visit (active users), and one line of totals per
            // day, Paris time, kept beyond the 15 days of Fly's metrics; plan O4 and O5: a
            // few values kept across restarts (release running, last weekly report)
            up: r#"
                ALTER TABLE users ADD COLUMN IF NOT EXISTS last_seen_at TIMESTAMPTZ;
                CREATE INDEX IF NOT EXISTS users_last_seen_at_idx ON users (last_seen_at);
                CREATE TABLE IF NOT EXISTS usage_days (
                    day DATE PRIMARY KEY,
                    active_users BIGINT NOT NULL DEFAULT 0,
                    photos_read BIGINT NOT NULL DEFAULT 0,
                    photos_failed BIGINT NOT NULL DEFAULT 0,
                    recipes_created BIGINT NOT NULL DEFAULT 0,
                    gemini_calls BIGINT NOT NULL DEFAULT 0,
                    gemini_failures BIGINT NOT NULL DEFAULT 0,
                    gemini_cost_micro_usd BIGINT NOT NULL DEFAULT 0
                );
                CREATE TABLE IF NOT EXISTS app_state (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL,
                    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
                );
            "#,
            down: Some(
                r#"
                DROP TABLE IF EXISTS app_state;
                DROP TABLE IF EXISTS usage_days;
                DROP INDEX IF EXISTS users_last_seen_at_idx;
                ALTER TABLE users DROP COLUMN IF EXISTS last_seen_at;
            "#,
            ),
        },
        Migration {
            version: 8,
            name: "dialogues",
            // Backlog IMP-022: each chat's running action (review, edit, rename) as JSON, so
            // a deploy or a restart does not lose it (`dialogue_storage`)
            up: r#"
                CREATE TABLE IF NOT EXISTS dialogues (
                    chat_id BIGINT PRIMARY KEY,
                    state JSON NOT NULL,
                    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
                );
            "#,
            down: Some(
                r#"
                DROP TABLE IF EXISTS dialogues;
            "#,
            ),
        },
    ]
}

/// Split SQL string into individual statements by semicolons
pub fn split_sql_statements(sql: &str) -> Result<Vec<String>, String> {
    let mut statements = Vec::new();
    let mut current_statement = String::new();
    let mut in_string = false;
    let mut string_char = '\0';
    let mut in_comment = false;
    let mut chars = sql.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            // Handle string literals
            '"' | '\'' if !in_comment => {
                if !in_string {
                    in_string = true;
                    string_char = ch;
                } else if ch == string_char {
                    in_string = false;
                    string_char = '\0';
                }
                current_statement.push(ch);
            }
            // Handle comments
            '-' if !in_string && !in_comment => {
                // Check if this is the start of a comment
                if chars.peek() == Some(&'-') {
                    in_comment = true;
                    current_statement.push(ch); // Push first -
                    match chars.next() {
                        Some(second_dash) => current_statement.push(second_dash), // Push second -
                        None => {
                            return Err("Unexpected end of input while parsing comment".to_string())
                        }
                    }
                } else {
                    current_statement.push(ch);
                }
            }
            '\n' if in_comment => {
                in_comment = false;
                current_statement.push(ch);
            }
            // Handle semicolons outside of strings and comments
            ';' if !in_string && !in_comment => {
                current_statement.push(ch);
                statements.push(current_statement.trim().to_string());
                current_statement = String::new();
            }
            _ => {
                current_statement.push(ch);
            }
        }
    }

    // Add any remaining statement
    if !current_statement.trim().is_empty() {
        statements.push(current_statement.trim().to_string());
    }

    Ok(statements)
}

/// Initialize the migrations table
pub async fn init_migrations_table(pool: &PgPool) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create migrations table")?;

    Ok(())
}

/// Get the current schema version
pub async fn get_current_version(pool: &PgPool) -> Result<i32> {
    let version: Option<i32> =
        sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM schema_migrations")
            .fetch_one(pool)
            .await
            .context("Failed to get current schema version")?;

    Ok(version.unwrap_or(0))
}

/// Apply pending migrations
pub async fn apply_pending_migrations(pool: &PgPool) -> Result<()> {
    init_migrations_table(pool).await?;

    let current_version = get_current_version(pool).await?;
    let migrations = get_migrations();

    for migration in migrations {
        if migration.version > current_version {
            info!(
                "Applying migration {}: {}",
                migration.version, migration.name
            );

            // Execute the migration in a transaction
            let mut tx = pool.begin().await.context(format!(
                "Failed to start transaction for migration {}",
                migration.version
            ))?;

            // Split the migration SQL into individual statements and execute each one
            let statements = split_sql_statements(migration.up).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to parse SQL for migration {}: {}",
                    migration.version,
                    e
                )
            })?;
            for statement in statements {
                if !statement.trim().is_empty() {
                    sqlx::query(&statement)
                        .execute(&mut *tx)
                        .await
                        .context(format!(
                            "Failed to execute statement in migration {}: {}",
                            migration.version, migration.name
                        ))?;
                }
            }

            // Record the migration
            sqlx::query("INSERT INTO schema_migrations (version, name) VALUES ($1, $2)")
                .bind(migration.version)
                .bind(migration.name)
                .execute(&mut *tx)
                .await
                .context(format!("Failed to record migration {}", migration.version))?;

            tx.commit()
                .await
                .context(format!("Failed to commit migration {}", migration.version))?;

            info!(
                "✓ Successfully applied migration {}: {}",
                migration.version, migration.name
            );
        }
    }

    let final_version = get_current_version(pool).await?;
    info!("Database schema is at version {}", final_version);

    Ok(())
}

/// Rollback to a specific version (for development/testing)
pub async fn rollback_to_version(pool: &PgPool, target_version: i32) -> Result<()> {
    let current_version = get_current_version(pool).await?;
    let migrations = get_migrations();

    if target_version >= current_version {
        info!(
            "Already at version {} (target: {})",
            current_version, target_version
        );
        return Ok(());
    }

    // Get migrations to rollback in reverse order
    let migrations_to_rollback: Vec<&Migration> = migrations
        .iter()
        .filter(|m| m.version > target_version)
        .collect();

    for migration in migrations_to_rollback.into_iter().rev() {
        if let Some(down_sql) = migration.down {
            info!(
                "Rolling back migration {}: {}",
                migration.version, migration.name
            );

            let mut tx = pool.begin().await.context(format!(
                "Failed to start transaction for rollback of migration {}",
                migration.version
            ))?;

            // Split the rollback SQL into individual statements and execute each one
            let statements = split_sql_statements(down_sql).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to parse rollback SQL for migration {}: {}",
                    migration.version,
                    e
                )
            })?;
            for statement in statements {
                if !statement.trim().is_empty() {
                    sqlx::query(&statement)
                        .execute(&mut *tx)
                        .await
                        .context(format!(
                            "Failed to execute rollback statement in migration {}: {}",
                            migration.version, migration.name
                        ))?;
                }
            }

            sqlx::query("DELETE FROM schema_migrations WHERE version = $1")
                .bind(migration.version)
                .execute(&mut *tx)
                .await
                .context(format!(
                    "Failed to remove migration record {}",
                    migration.version
                ))?;

            tx.commit().await.context(format!(
                "Failed to commit rollback of migration {}",
                migration.version
            ))?;

            info!(
                "✓ Successfully rolled back migration {}: {}",
                migration.version, migration.name
            );
        } else {
            return Err(anyhow::anyhow!(
                "Cannot rollback migration {}: no down migration defined",
                migration.version
            ));
        }
    }

    let final_version = get_current_version(pool).await?;
    info!("Database schema rolled back to version {}", final_version);

    Ok(())
}
