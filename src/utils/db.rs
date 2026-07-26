/*
 * Copyright (C) 2026 VeryBaaad <verybaaad@outlook.com>
 * SPDX-License-Identifier: MIT
 */
use crate::utils::DbPool;
use crate::utils::config::DatabaseKind;
use crate::utils::logger::log;
use chrono::Duration;
use log::Level;
use sqlx::{Any, Row};
use std::error::Error;
use teloxide::{prelude::*, utils::markdown};

const CURRENT_DB_VERSION: i32 = 6;

/// Treat an absent and an empty username as the same thing, so a user without a Telegram
/// username always lands in the database as NULL rather than an empty string.
fn normalize_username(username: Option<&str>) -> Option<&str> {
    username.filter(|s| !s.is_empty())
}

/// Compact user identity used to pass user fields to DB/service functions.
pub struct UserIdent<'a> {
    pub user_id: i64,
    pub username: Option<&'a str>,
    pub first_name: Option<&'a str>,
    pub last_name: Option<&'a str>,
}

fn users_table_exists_sql(kind: DatabaseKind) -> &'static str {
    match kind {
        DatabaseKind::Sqlite => {
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name = ?)"
        }
        DatabaseKind::Postgres => {
            "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_schema = current_schema() AND table_name = $1)"
        }
        DatabaseKind::MySql | DatabaseKind::MariaDb => {
            "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_schema = DATABASE() AND table_name = ?)"
        }
    }
}

fn users_table_ddl(kind: DatabaseKind) -> &'static str {
    match kind {
        DatabaseKind::Sqlite => {
            "CREATE TABLE IF NOT EXISTS users (
                user_id INTEGER NOT NULL UNIQUE,
                username TEXT,
                first_name TEXT,
                last_name TEXT,
                count INTEGER NOT NULL DEFAULT 0,
                last_time BIGINT NOT NULL DEFAULT 0,
                is_admin BOOLEAN NOT NULL DEFAULT 0,
                is_banned INTEGER NOT NULL DEFAULT 0,
                probably_guarantee INTEGER NOT NULL DEFAULT 0
            )"
        }
        DatabaseKind::Postgres => {
            "CREATE TABLE IF NOT EXISTS users (
                user_id BIGINT NOT NULL UNIQUE,
                username TEXT,
                first_name TEXT,
                last_name TEXT,
                count BIGINT NOT NULL DEFAULT 0,
                last_time BIGINT NOT NULL DEFAULT 0,
                is_admin BOOLEAN NOT NULL DEFAULT FALSE,
                is_banned INTEGER NOT NULL DEFAULT 0,
                probably_guarantee INTEGER NOT NULL DEFAULT 0
            )"
        }
        DatabaseKind::MySql | DatabaseKind::MariaDb => {
            "CREATE TABLE IF NOT EXISTS users (
                user_id BIGINT NOT NULL UNIQUE,
                username TEXT,
                first_name TEXT,
                last_name TEXT,
                count BIGINT NOT NULL DEFAULT 0,
                last_time BIGINT NOT NULL DEFAULT 0,
                is_admin BOOLEAN NOT NULL DEFAULT FALSE,
                is_banned INTEGER NOT NULL DEFAULT 0,
                probably_guarantee INTEGER NOT NULL DEFAULT 0
            )"
        }
    }
}

fn column_exists_sql(kind: DatabaseKind) -> &'static str {
    match kind {
        DatabaseKind::Sqlite => {
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('users') WHERE name = ?)"
        }
        DatabaseKind::Postgres => {
            "SELECT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema = current_schema() AND table_name = 'users' AND column_name = $1)"
        }
        DatabaseKind::MySql | DatabaseKind::MariaDb => {
            "SELECT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema = DATABASE() AND table_name = 'users' AND column_name = ?)"
        }
    }
}

fn add_is_admin_sql(kind: DatabaseKind) -> &'static str {
    match kind {
        DatabaseKind::Sqlite => "ALTER TABLE users ADD COLUMN is_admin BOOLEAN NOT NULL DEFAULT 0",
        DatabaseKind::Postgres | DatabaseKind::MySql | DatabaseKind::MariaDb => {
            "ALTER TABLE users ADD COLUMN is_admin BOOLEAN NOT NULL DEFAULT FALSE"
        }
    }
}

fn add_is_banned_sql(kind: DatabaseKind) -> &'static str {
    match kind {
        DatabaseKind::Sqlite
        | DatabaseKind::Postgres
        | DatabaseKind::MySql
        | DatabaseKind::MariaDb => {
            "ALTER TABLE users ADD COLUMN is_banned INTEGER NOT NULL DEFAULT 0"
        }
    }
}

fn add_first_name_sql(_kind: DatabaseKind) -> &'static str {
    "ALTER TABLE users ADD COLUMN first_name TEXT"
}

fn add_last_name_sql(_kind: DatabaseKind) -> &'static str {
    "ALTER TABLE users ADD COLUMN last_name TEXT"
}

fn add_probably_guarantee_sql(_kind: DatabaseKind) -> &'static str {
    "ALTER TABLE users ADD COLUMN probably_guarantee INTEGER NOT NULL DEFAULT 0"
}

/// Telegram usernames are optional, so the column must accept NULL.
/// SQLite cannot drop a NOT NULL constraint in place and is handled by a table rebuild instead.
fn drop_username_not_null_sql(kind: DatabaseKind) -> &'static str {
    match kind {
        DatabaseKind::Sqlite => unreachable!("SQLite uses a table rebuild to drop NOT NULL"),
        DatabaseKind::Postgres => "ALTER TABLE users ALTER COLUMN username DROP NOT NULL",
        DatabaseKind::MySql | DatabaseKind::MariaDb => {
            "ALTER TABLE users MODIFY COLUMN username TEXT NULL"
        }
    }
}

/// Rows written before the column became nullable stored an empty string instead of NULL.
fn normalize_empty_username_sql() -> &'static str {
    "UPDATE users SET username = NULL WHERE username = ''"
}

fn upsert_user_sql(kind: DatabaseKind) -> &'static str {
    match kind {
        DatabaseKind::Sqlite => {
            "INSERT INTO users (user_id, username, first_name, last_name, count, last_time) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(user_id) DO UPDATE SET username = excluded.username, first_name = excluded.first_name, last_name = excluded.last_name, count = excluded.count, last_time = excluded.last_time"
        }
        DatabaseKind::Postgres => {
            "INSERT INTO users (user_id, username, first_name, last_name, count, last_time) VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT(user_id) DO UPDATE SET username = excluded.username, first_name = excluded.first_name, last_name = excluded.last_name, count = excluded.count, last_time = excluded.last_time"
        }
        DatabaseKind::MySql | DatabaseKind::MariaDb => {
            "INSERT INTO users (user_id, username, first_name, last_name, count, last_time) VALUES (?, ?, ?, ?, ?, ?) ON DUPLICATE KEY UPDATE username = VALUES(username), first_name = VALUES(first_name), last_name = VALUES(last_name), count = VALUES(count), last_time = VALUES(last_time)"
        }
    }
}

fn get_rank_sql(kind: DatabaseKind) -> &'static str {
    match kind {
        DatabaseKind::Sqlite => {
            "SELECT COUNT(*) as user_rank FROM users WHERE count > (SELECT count FROM users WHERE user_id = ?) OR (count = (SELECT count FROM users WHERE user_id = ?) AND last_time < (SELECT last_time FROM users WHERE user_id = ?))"
        }
        DatabaseKind::Postgres => {
            "SELECT COUNT(*) as user_rank FROM users WHERE \"count\" > (SELECT \"count\" FROM users WHERE user_id = $1) OR (\"count\" = (SELECT \"count\" FROM users WHERE user_id = $2) AND last_time < (SELECT last_time FROM users WHERE user_id = $3))"
        }
        DatabaseKind::MySql | DatabaseKind::MariaDb => {
            "SELECT COUNT(*) as `user_rank` FROM users WHERE `count` > (SELECT `count` FROM users WHERE user_id = ?) OR (`count` = (SELECT `count` FROM users WHERE user_id = ?) AND last_time < (SELECT last_time FROM users WHERE user_id = ?))"
        }
    }
}

fn insert_db_version_sql(kind: DatabaseKind) -> &'static str {
    match kind {
        DatabaseKind::Sqlite | DatabaseKind::MySql | DatabaseKind::MariaDb => {
            "INSERT INTO db_version (version) VALUES (?)"
        }
        DatabaseKind::Postgres => "INSERT INTO db_version (version) VALUES ($1)",
    }
}

fn update_db_version_sql(kind: DatabaseKind) -> &'static str {
    match kind {
        DatabaseKind::Sqlite | DatabaseKind::MySql | DatabaseKind::MariaDb => {
            "UPDATE db_version SET version = ?"
        }
        DatabaseKind::Postgres => "UPDATE db_version SET version = $1",
    }
}

/// Read an `EXISTS(...)` result as bool, handling type differences across databases.
/// SQLite returns integer, MySQL/MariaDB return BIGINT, Postgres returns bool.
fn exists_result_to_bool(row: &sqlx::any::AnyRow, kind: DatabaseKind) -> Result<bool, sqlx::Error> {
    match kind {
        DatabaseKind::Postgres => row.try_get(0),
        _ => {
            let val: i64 = row.try_get(0)?;
            Ok(val != 0)
        }
    }
}

pub async fn init_database(pool: &DbPool, database_kind: DatabaseKind) {
    sqlx::query("CREATE TABLE IF NOT EXISTS db_version (version INTEGER NOT NULL)")
        .execute(pool)
        .await
        .expect("Failed to create db_version table");

    let current_version: Option<i32> = sqlx::query("SELECT version FROM db_version")
        .fetch_optional(pool)
        .await
        .expect("Failed to query db_version")
        .map(|row| row.get("version"));

    let version = match current_version {
        Some(v) => {
            if v > CURRENT_DB_VERSION {
                log(
                    Level::Error,
                    "init_database",
                    &format!(
                        "Database version ({}) is higher than expected ({}), please upgrade the program",
                        v, CURRENT_DB_VERSION
                    ),
                );
                panic!(
                    "Database version ({}) is higher than expected ({}), please upgrade the program",
                    v, CURRENT_DB_VERSION
                );
            }
            v
        }
        None => {
            let row = sqlx::query(users_table_exists_sql(database_kind))
                .bind("users")
                .fetch_one(pool)
                .await
                .expect("Failed to check users table");
            let users_exists =
                exists_result_to_bool(&row, database_kind).expect("Failed to get EXISTS result");

            if users_exists {
                log(
                    Level::Info,
                    "init_database",
                    "Detected legacy database (v0), inserting version and starting migration",
                );
                sqlx::query("INSERT INTO db_version (version) VALUES (0)")
                    .execute(pool)
                    .await
                    .expect("Failed to insert initial version");
                0
            } else {
                log(
                    Level::Info,
                    "init_database",
                    "Detected fresh database, creating tables and setting version",
                );
                sqlx::query(users_table_ddl(database_kind))
                    .execute(pool)
                    .await
                    .expect("Failed to create users table");
                sqlx::query(insert_db_version_sql(database_kind))
                    .bind(CURRENT_DB_VERSION)
                    .execute(pool)
                    .await
                    .expect("Failed to insert version");
                log(
                    Level::Info,
                    "init_database",
                    &format!("Database initialized at version {}", CURRENT_DB_VERSION),
                );
                return;
            }
        }
    };

    if version < CURRENT_DB_VERSION {
        upgrade_database(pool, version, database_kind).await;
    } else {
        log(
            Level::Info,
            "init_database",
            &format!("Database version: {}, already up to date", version),
        );
    }

    repair_null_last_time(pool)
        .await
        .expect("Failed to backfill null last_time values");
}

async fn column_exists(
    pool: &DbPool,
    database_kind: DatabaseKind,
    column_name: &str,
) -> Result<bool, sqlx::Error> {
    let row = sqlx::query(column_exists_sql(database_kind))
        .bind(column_name)
        .fetch_one(pool)
        .await?;
    exists_result_to_bool(&row, database_kind)
}

/// Migrate from the given version up to CURRENT_DB_VERSION
async fn upgrade_database(pool: &DbPool, from_version: i32, database_kind: DatabaseKind) {
    let mut v = from_version;

    if v == 0 {
        log(
            Level::Info,
            "init_database",
            "Running migration v0 -> v1: adding is_admin column",
        );
        if !column_exists(pool, database_kind, "is_admin")
            .await
            .unwrap_or(true)
        {
            sqlx::query(add_is_admin_sql(database_kind))
                .execute(pool)
                .await
                .expect("Failed to add is_admin column");
        }
        v = 1;
    }
    if v == 1 {
        log(
            Level::Info,
            "init_database",
            "Running migration v1 -> v2: adding is_banned column",
        );
        if !column_exists(pool, database_kind, "is_banned")
            .await
            .unwrap_or(true)
        {
            sqlx::query(add_is_banned_sql(database_kind))
                .execute(pool)
                .await
                .expect("Failed to add is_banned column");
        }
        v = 2;
    }

    if v == 2 {
        log(
            Level::Info,
            "init_database",
            "Running migration v2 -> v3: converting last_time to unix seconds",
        );
        match database_kind {
            DatabaseKind::Sqlite => {
                migrate_sqlite_last_time_to_unix(pool)
                    .await
                    .expect("Failed to convert last_time column");
            }
            DatabaseKind::Postgres | DatabaseKind::MySql | DatabaseKind::MariaDb => {
                log(
                    Level::Info,
                    "init_database",
                    "No schema rewrite needed for this backend, only bumping version",
                );
            }
        }
        v = 3;
    }

    if v == 3 {
        log(
            Level::Info,
            "init_database",
            "Running migration v3 -> v4: adding first_name and last_name columns",
        );
        if !column_exists(pool, database_kind, "first_name")
            .await
            .unwrap_or(true)
        {
            sqlx::query(add_first_name_sql(database_kind))
                .execute(pool)
                .await
                .expect("Failed to add first_name column");
        }
        if !column_exists(pool, database_kind, "last_name")
            .await
            .unwrap_or(true)
        {
            sqlx::query(add_last_name_sql(database_kind))
                .execute(pool)
                .await
                .expect("Failed to add last_name column");
        }
        v = 4;
    }

    if v == 4 {
        log(
            Level::Info,
            "init_database",
            "Running migration v4 -> v5: adding probably_guarantee column",
        );
        if !column_exists(pool, database_kind, "probably_guarantee")
            .await
            .unwrap_or(true)
        {
            sqlx::query(add_probably_guarantee_sql(database_kind))
                .execute(pool)
                .await
                .expect("Failed to add probably_guarantee column");
        }
        v = 5;
    }

    if v == 5 {
        log(
            Level::Info,
            "init_database",
            "Running migration v5 -> v6: making username nullable",
        );
        match database_kind {
            DatabaseKind::Sqlite => {
                migrate_sqlite_username_nullable(pool)
                    .await
                    .expect("Failed to make username column nullable");
            }
            DatabaseKind::Postgres | DatabaseKind::MySql | DatabaseKind::MariaDb => {
                sqlx::query(drop_username_not_null_sql(database_kind))
                    .execute(pool)
                    .await
                    .expect("Failed to drop NOT NULL on username column");
                sqlx::query(normalize_empty_username_sql())
                    .execute(pool)
                    .await
                    .expect("Failed to normalize empty usernames");
            }
        }
        v = 6;
    }

    sqlx::query(update_db_version_sql(database_kind))
        .bind(v)
        .execute(pool)
        .await
        .expect("Failed to update db_version");
    log(
        Level::Info,
        "init_database",
        &format!("Database migration complete: {} -> {}", from_version, v),
    );
}

async fn migrate_sqlite_last_time_to_unix(pool: &DbPool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query("ALTER TABLE users RENAME TO users_v2")
        .execute(&mut *tx)
        .await?;

    // `username` stays nullable here: legacy databases predate the NOT NULL constraint and may
    // already hold NULL usernames, which this rebuild would otherwise reject. The v5 -> v6
    // migration relaxes the constraint everywhere anyway.
    sqlx::query(
        "CREATE TABLE users (
            user_id INTEGER NOT NULL UNIQUE,
            username TEXT,
            count INTEGER NOT NULL DEFAULT 0,
            last_time BIGINT,
            is_admin BOOLEAN NOT NULL DEFAULT 0,
            is_banned INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO users (user_id, username, count, last_time, is_admin, is_banned)
         SELECT
             user_id,
             username,
             count,
             CASE
                 WHEN last_time IS NULL THEN 0
                 WHEN typeof(last_time) = 'integer' THEN last_time
                 ELSE CAST(strftime('%s', last_time) AS INTEGER)
             END,
             is_admin,
             is_banned
         FROM users_v2",
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query("DROP TABLE users_v2").execute(&mut *tx).await?;

    tx.commit().await?;
    Ok(())
}

async fn migrate_sqlite_username_nullable(pool: &DbPool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query("ALTER TABLE users RENAME TO users_v5")
        .execute(&mut *tx)
        .await?;

    sqlx::query(users_table_ddl(DatabaseKind::Sqlite))
        .execute(&mut *tx)
        .await?;

    // NULLIF turns the empty-string placeholder written before this migration into a real NULL,
    // COALESCE covers last_time rows still nullable from the v2 -> v3 rebuild.
    sqlx::query(
        "INSERT INTO users (user_id, username, first_name, last_name, count, last_time, is_admin, is_banned, probably_guarantee)
         SELECT
             user_id,
             NULLIF(username, ''),
             first_name,
             last_name,
             count,
             COALESCE(last_time, 0),
             is_admin,
             is_banned,
             probably_guarantee
         FROM users_v5",
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query("DROP TABLE users_v5").execute(&mut *tx).await?;

    tx.commit().await?;
    Ok(())
}

async fn repair_null_last_time(pool: &DbPool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET last_time = 0 WHERE last_time IS NULL")
        .execute(pool)
        .await?;
    Ok(())
}

/// Check if a user is an admin
/// Check if a user is an admin
pub async fn is_admin(
    pool: &DbPool,
    database_kind: DatabaseKind,
    user_id: i64,
) -> Result<bool, sqlx::Error> {
    // Use CAST to handle MySQL TINYINT(1) / BOOLEAN type with sqlx::Any
    let query_str = match database_kind {
        DatabaseKind::Sqlite | DatabaseKind::MySql | DatabaseKind::MariaDb => {
            "SELECT CAST(is_admin AS SIGNED) as is_admin_val FROM users WHERE user_id = ?"
        }
        DatabaseKind::Postgres => {
            "SELECT CAST(is_admin AS INTEGER) as is_admin_val FROM users WHERE user_id = $1"
        }
    };
    let row = sqlx::query(query_str)
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

    log(
        Level::Debug,
        "is_admin",
        &format!(
            "Checking is_admin for user {}: row_found={}",
            user_id,
            row.is_some()
        ),
    );

    Ok(row
        .and_then(|r| match r.try_get::<i32, _>("is_admin_val") {
            Ok(v) => {
                let b = v != 0;
                log(
                    Level::Debug,
                    "is_admin",
                    &format!("User {} is_admin: {} (from value {})", user_id, b, v),
                );
                Some(b)
            }
            Err(e) => {
                log(
                    Level::Error,
                    "is_admin",
                    &format!("Failed to get is_admin for user {}: {}", user_id, e),
                );
                None
            }
        })
        .unwrap_or(false))
}

// Ban Status
pub async fn ban_status(
    pool: &DbPool,
    database_kind: DatabaseKind,
    user_id: i64,
) -> Result<i32, sqlx::Error> {
    sqlx::query_scalar(match database_kind {
        DatabaseKind::Sqlite | DatabaseKind::MySql | DatabaseKind::MariaDb => {
            "SELECT is_banned FROM users WHERE user_id = ?"
        }
        DatabaseKind::Postgres => "SELECT is_banned FROM users WHERE user_id = $1",
    })
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map(|opt| opt.unwrap_or(0))
}

/// Set a user's count
pub async fn set_user_count(
    pool: &DbPool,
    database_kind: DatabaseKind,
    user_id: i64,
    count: i64,
) -> Result<(), sqlx::Error> {
    log(
        Level::Info,
        "set_user_count",
        &format!("Setting user {} count to {}", user_id, count),
    );
    sqlx::query(match database_kind {
        DatabaseKind::Sqlite | DatabaseKind::MySql | DatabaseKind::MariaDb => {
            "UPDATE users SET count = ? WHERE user_id = ?"
        }
        DatabaseKind::Postgres => "UPDATE users SET count = $1 WHERE user_id = $2",
    })
    .bind(count)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Set a user's last_time to 0
pub async fn set_user_last_time(
    pool: &DbPool,
    database_kind: DatabaseKind,
    user_id: i64,
) -> Result<(), sqlx::Error> {
    log(
        Level::Info,
        "set_user_last_time",
        &format!("Setting user {} last_time to 0", user_id),
    );
    sqlx::query(match database_kind {
        DatabaseKind::Sqlite | DatabaseKind::MySql | DatabaseKind::MariaDb => {
            "UPDATE users SET last_time = 0 WHERE user_id = ?"
        }
        DatabaseKind::Postgres => "UPDATE users SET last_time = 0 WHERE user_id = $1",
    })
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete a user from the table
pub async fn delete_user(
    pool: &DbPool,
    database_kind: DatabaseKind,
    user_id: i64,
) -> Result<(), sqlx::Error> {
    log(
        Level::Info,
        "delete_user",
        &format!("Deleting user {}", user_id),
    );
    sqlx::query(match database_kind {
        DatabaseKind::Sqlite | DatabaseKind::MySql | DatabaseKind::MariaDb => {
            "DELETE FROM users WHERE user_id = ?"
        }
        DatabaseKind::Postgres => "DELETE FROM users WHERE user_id = $1",
    })
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn upsert_user<'a, E>(
    pool: E,
    database_kind: DatabaseKind,
    user: &UserIdent<'_>,
    new_count: i64,
    now: i64,
) -> Result<(), sqlx::Error>
where
    E: sqlx::Executor<'a, Database = Any>,
{
    log(
        Level::Debug,
        "handle_zw",
        "Inserting/updating user in database",
    );
    if let Err(e) = sqlx::query(upsert_user_sql(database_kind))
        .bind(user.user_id)
        .bind(normalize_username(user.username))
        .bind(user.first_name)
        .bind(user.last_name)
        .bind(new_count)
        .bind(now)
        .execute(pool)
        .await
    {
        log(
            Level::Error,
            "handle_zw",
            &format!("Failed to update user in database: {}", e),
        );
        return Err(e);
    }
    log(Level::Debug, "handle_zw", "Database update successful");
    Ok(())
}

pub async fn user_exists(
    pool: &DbPool,
    database_kind: DatabaseKind,
    user_id: i64,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    log(
        Level::Debug,
        "user_exists",
        &format!("Checking if user {} exists", user_id),
    );
    let row = sqlx::query(match database_kind {
        DatabaseKind::Sqlite | DatabaseKind::MySql | DatabaseKind::MariaDb => {
            "SELECT user_id FROM users WHERE user_id = ?"
        }
        DatabaseKind::Postgres => "SELECT user_id FROM users WHERE user_id = $1",
    })
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

pub async fn get_total_users(
    pool: &DbPool,
    database_kind: DatabaseKind,
) -> Result<i64, Box<dyn Error + Send + Sync>> {
    log(Level::Debug, "get_total_users", "Fetching total user count");
    let row = sqlx::query(match database_kind {
        DatabaseKind::Sqlite => "SELECT COUNT(*) as user_count FROM users",
        DatabaseKind::Postgres => "SELECT COUNT(*) as user_count FROM users",
        DatabaseKind::MySql | DatabaseKind::MariaDb => "SELECT COUNT(*) as `user_count` FROM users",
    })
    .fetch_one(pool)
    .await?;
    let count: i64 = row.try_get("user_count")?;
    Ok(count)
}

pub async fn get_rank(
    pool: &DbPool,
    user_id: i64,
    database_kind: DatabaseKind,
) -> Result<usize, Box<dyn Error + Send + Sync>> {
    log(
        Level::Debug,
        "get_rank",
        &format!("Calculating rank for user: {}", user_id),
    );
    let row = match sqlx::query(get_rank_sql(database_kind))
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .fetch_one(pool)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log(
                Level::Error,
                "get_rank",
                &format!("Failed to fetch rank for user {}: {}", user_id, e),
            );
            return Err(Box::new(e));
        }
    };
    let rank: i64 = row.try_get("user_rank")?;
    let final_rank = (rank + 1) as usize;
    log(
        Level::Debug,
        "get_rank",
        &format!("User {} rank: {}", user_id, final_rank),
    );
    Ok(final_rank)
}

pub async fn get_user_count_and_last_time(
    pool: &DbPool,
    database_kind: DatabaseKind,
    user_id: i64,
) -> Result<(i64, Option<i64>), Box<dyn Error + Send + Sync>> {
    log(
        Level::Debug,
        "get_user_count_and_last_time",
        &format!("Fetching count and last_time for user {}", user_id),
    );
    let row = sqlx::query(match database_kind {
        DatabaseKind::Sqlite | DatabaseKind::MySql | DatabaseKind::MariaDb => {
            "SELECT count, last_time FROM users WHERE user_id = ?"
        }
        DatabaseKind::Postgres => "SELECT count, last_time FROM users WHERE user_id = $1",
    })
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    if let Some(row) = row {
        let count: i64 = row.try_get("count")?;
        let last_time: Option<i64> = row.try_get("last_time").ok();
        Ok((count, last_time))
    } else {
        Ok((0, None))
    }
}

/// Find user by ID or username, returns (count, last_time, username, first_name, last_name, user_id)
pub async fn find_user_by_id_or_username(
    pool: &DbPool,
    database_kind: DatabaseKind,
    key: &str,
) -> Result<
    Option<(
        i64,
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
    )>,
    Box<dyn Error + Send + Sync>,
> {
    log(
        Level::Debug,
        "find_user_by_id_or_username",
        &format!("Searching for user by key: {}", key),
    );

    let (sql_by_id, sql_by_name) = match database_kind {
        DatabaseKind::Sqlite | DatabaseKind::MySql | DatabaseKind::MariaDb => (
            "SELECT count, last_time, username, first_name, last_name, user_id FROM users WHERE user_id = ?",
            "SELECT count, last_time, username, first_name, last_name, user_id FROM users WHERE username = ?",
        ),
        DatabaseKind::Postgres => (
            "SELECT count, last_time, username, first_name, last_name, user_id FROM users WHERE user_id = $1",
            "SELECT count, last_time, username, first_name, last_name, user_id FROM users WHERE username = $1",
        ),
    };

    if let Ok(id) = key.parse::<i64>() {
        if let Some(row) = sqlx::query(sql_by_id).bind(id).fetch_optional(pool).await? {
            let count: i64 = row.try_get("count")?;
            let last_time: Option<i64> = row.try_get("last_time").ok();
            let username: Option<String> = row.try_get("username").unwrap_or(None);
            let first_name: Option<String> = row.try_get("first_name").ok();
            let last_name: Option<String> = row.try_get("last_name").ok();
            let user_id: i64 = row.try_get("user_id")?;
            return Ok(Some((
                count, last_time, username, first_name, last_name, user_id,
            )));
        }
        return Ok(None);
    }

    let uname = key.trim_start_matches('@');
    // An empty key would otherwise match legacy rows that still hold an empty username.
    if uname.is_empty() {
        return Ok(None);
    }
    if let Some(row) = sqlx::query(sql_by_name)
        .bind(uname)
        .fetch_optional(pool)
        .await?
    {
        let count: i64 = row.try_get("count")?;
        let last_time: Option<i64> = row.try_get("last_time").ok();
        let username: Option<String> = row.try_get("username").unwrap_or(None);
        let first_name: Option<String> = row.try_get("first_name").ok();
        let last_name: Option<String> = row.try_get("last_name").ok();
        let user_id: i64 = row.try_get("user_id")?;
        Ok(Some((
            count, last_time, username, first_name, last_name, user_id,
        )))
    } else {
        Ok(None)
    }
}

/// Sync user info (username, first_name, last_name) from a Telegram user to the database.
/// Updates any fields that differ from stored values, and returns whether any update was made.
pub async fn sync_user_info(
    pool: &DbPool,
    database_kind: DatabaseKind,
    user_id: i64,
    username: Option<&str>,
    first_name: Option<&str>,
    last_name: Option<&str>,
) -> Result<(), sqlx::Error> {
    let row = sqlx::query(match database_kind {
        DatabaseKind::Sqlite | DatabaseKind::MySql | DatabaseKind::MariaDb => {
            "SELECT username, first_name, last_name FROM users WHERE user_id = ?"
        }
        DatabaseKind::Postgres => {
            "SELECT username, first_name, last_name FROM users WHERE user_id = $1"
        }
    })
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    if let Some(row) = row {
        let stored_username: Option<String> = row.try_get("username").unwrap_or(None);
        let stored_first_name: Option<String> = row.try_get("first_name").ok();
        let stored_last_name: Option<String> = row.try_get("last_name").ok();

        let new_username = normalize_username(username);
        let username_changed = normalize_username(stored_username.as_deref()) != new_username;
        let first_name_changed = stored_first_name.as_deref() != first_name;
        let last_name_changed = stored_last_name.as_deref() != last_name;

        if username_changed || first_name_changed || last_name_changed {
            log(
                Level::Debug,
                "sync_user_info",
                &format!(
                    "Updating user info for {}: username={}, fn={}, ln={})",
                    user_id,
                    if username_changed {
                        "changed"
                    } else {
                        "unchanged"
                    },
                    if first_name_changed {
                        "changed"
                    } else {
                        "unchanged"
                    },
                    if last_name_changed {
                        "changed"
                    } else {
                        "unchanged"
                    },
                ),
            );
            sqlx::query(match database_kind {
                DatabaseKind::Sqlite | DatabaseKind::MySql | DatabaseKind::MariaDb => {
                    "UPDATE users SET username = ?, first_name = ?, last_name = ? WHERE user_id = ?"
                }
                DatabaseKind::Postgres => {
                    "UPDATE users SET username = $1, first_name = $2, last_name = $3 WHERE user_id = $4"
                }
            })
            .bind(new_username)
            .bind(first_name)
            .bind(last_name)
            .bind(user_id)
            .execute(pool)
            .await?;
        }
    } else {
        // User doesn't exist yet, insert a placeholder row (count=0, last_time=0)
        log(
            Level::Debug,
            "sync_user_info",
            &format!("User {} not in DB, inserting placeholder", user_id),
        );
        upsert_user(
            pool,
            database_kind,
            &UserIdent {
                user_id,
                username,
                first_name,
                last_name,
            },
            0,
            0,
        )
        .await?;
    }

    Ok(())
}

/// Build the best display name for a user with automatic fallback.
/// Priority: full name (first_name + last_name, Telegram style) → username → user_id
pub fn get_user_display_name(
    first_name: Option<&str>,
    last_name: Option<&str>,
    username: Option<&str>,
    user_id: i64,
) -> String {
    let full_name = match (
        first_name.filter(|s| !s.is_empty()),
        last_name.filter(|s| !s.is_empty()),
    ) {
        (Some(f), Some(l)) => Some(format!("{} {}", f, l)),
        (Some(f), None) => Some(f.to_string()),
        (None, Some(l)) => Some(l.to_string()),
        (None, None) => None,
    };

    if let Some(name) = full_name {
        name
    } else if let Some(uname) = username.filter(|s| !s.is_empty()) {
        format!("@{}", uname)
    } else {
        user_id.to_string()
    }
}

/// Format a user mention link for MarkdownV2 with automatic fallback.
/// Priority: full name (first_name + last_name) → username → user_id
/// The returned string is already escaped and wrapped in a markdown user mention link.
pub fn format_user_mention(
    user_id: i64,
    first_name: Option<&str>,
    last_name: Option<&str>,
    username: Option<&str>,
) -> String {
    let display = get_user_display_name(first_name, last_name, username, user_id);
    markdown::user_mention(UserId(user_id as u64), &markdown::escape(&display))
}

/// Cooldown status check result
#[derive(Debug, Clone)]
pub struct CooldownStatus {
    pub is_in_cooldown: bool,
    pub mins: i64,
    pub secs: i64,
}

/// Check cooldown status for a user
pub fn check_cooldown(last_time: Option<i64>, now: i64, duration: Duration) -> CooldownStatus {
    if let Some(lt) = last_time {
        let next_time = lt + duration.num_seconds();
        if now < next_time {
            let remaining = next_time - now;
            let mins = remaining / 60;
            let secs = remaining % 60;
            return CooldownStatus {
                is_in_cooldown: true,
                mins,
                secs,
            };
        }
    }
    CooldownStatus {
        is_in_cooldown: false,
        mins: 0,
        secs: 0,
    }
}

pub async fn get_probably_guarantee(
    pool: &DbPool,
    database_kind: DatabaseKind,
    user_id: i64,
) -> Result<i64, Box<dyn Error + Send + Sync>> {
    log(
        Level::Debug,
        "get_probably_guarantee",
        &format!("Fetching probably_guarantee for user {}", user_id),
    );
    let row = sqlx::query(match database_kind {
        DatabaseKind::Sqlite | DatabaseKind::MySql | DatabaseKind::MariaDb => {
            "SELECT probably_guarantee FROM users WHERE user_id = ?"
        }
        DatabaseKind::Postgres => "SELECT probably_guarantee FROM users WHERE user_id = $1",
    })
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    if let Some(row) = row {
        let count: i64 = row.try_get("probably_guarantee")?;
        Ok(count)
    } else {
        Ok(0)
    }
}

pub async fn set_probably_guarantee(
    pool: &DbPool,
    database_kind: DatabaseKind,
    user_id: i64,
    probably_guarantee: i64,
) -> Result<(), sqlx::Error> {
    log(
        Level::Info,
        "set_probably_guarantee",
        &format!(
            "Setting user {} probably_guarantee to {}",
            user_id, probably_guarantee
        ),
    );
    sqlx::query(match database_kind {
        DatabaseKind::Sqlite | DatabaseKind::MySql | DatabaseKind::MariaDb => {
            "UPDATE users SET probably_guarantee = ? WHERE user_id = ?"
        }
        DatabaseKind::Postgres => "UPDATE users SET probably_guarantee = $1 WHERE user_id = $2",
    })
    .bind(probably_guarantee)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::any::install_default_drivers;
    use std::sync::Once;

    static DRIVERS: Once = Once::new();

    /// Create an empty SQLite file and connect to it, mirroring what main.rs does at startup
    /// (sqlx::Any does not auto-create missing SQLite files).
    async fn fresh_sqlite(name: &str) -> (DbPool, std::path::PathBuf) {
        DRIVERS.call_once(install_default_drivers);
        let path = std::env::temp_dir().join(format!("zw-rs-migration-test-{}.db", name));
        let _ = std::fs::remove_file(&path);
        std::fs::File::create(&path).expect("failed to create test database file");
        let pool = DbPool::connect(&format!("sqlite:{}", path.display()))
            .await
            .expect("failed to connect to test database");
        (pool, path)
    }

    async fn db_version(pool: &DbPool) -> i32 {
        sqlx::query_scalar("SELECT version FROM db_version")
            .fetch_one(pool)
            .await
            .expect("failed to read db_version")
    }

    async fn username_of(pool: &DbPool, user_id: i64) -> Option<String> {
        sqlx::query_scalar("SELECT username FROM users WHERE user_id = ?")
            .bind(user_id)
            .fetch_one(pool)
            .await
            .expect("failed to read username")
    }

    /// A legacy database matching the schema documented in README.md, where `username`
    /// was already nullable and `last_time` was a DATETIME string.
    #[tokio::test]
    async fn legacy_v0_with_null_username_migrates_to_v6() {
        let (pool, path) = fresh_sqlite("legacy-v0").await;
        sqlx::query(
            "CREATE TABLE users (
                id INTEGER PRIMARY KEY,
                user_id INTEGER UNIQUE,
                username TEXT,
                count INTEGER DEFAULT 0,
                last_time DATETIME
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO users (user_id, username, count, last_time) VALUES
                (1, 'alice', 10, '2023-11-14 22:13:20'),
                (2, NULL, 5, NULL),
                (3, '', 7, '2023-11-14 22:13:21')",
        )
        .execute(&pool)
        .await
        .unwrap();

        init_database(&pool, DatabaseKind::Sqlite).await;

        assert_eq!(db_version(&pool).await, CURRENT_DB_VERSION);
        assert_eq!(username_of(&pool, 1).await.as_deref(), Some("alice"));
        assert_eq!(username_of(&pool, 2).await, None);
        assert_eq!(username_of(&pool, 3).await, None, "'' must become NULL");

        let (count, last_time) = get_user_count_and_last_time(&pool, DatabaseKind::Sqlite, 1)
            .await
            .unwrap();
        assert_eq!(count, 10);
        assert_eq!(last_time, Some(1700000000));

        let _ = std::fs::remove_file(path);
    }

    /// A database created by v2.2.2 as a fresh install: v5 column order, username NOT NULL.
    #[tokio::test]
    async fn fresh_v5_migrates_to_v6_and_accepts_null_username() {
        let (pool, path) = fresh_sqlite("fresh-v5").await;
        sqlx::query(
            "CREATE TABLE users (
                user_id INTEGER NOT NULL UNIQUE,
                username TEXT NOT NULL,
                first_name TEXT,
                last_name TEXT,
                count INTEGER NOT NULL DEFAULT 0,
                last_time BIGINT NOT NULL DEFAULT 0,
                is_admin BOOLEAN NOT NULL DEFAULT 0,
                is_banned INTEGER NOT NULL DEFAULT 0,
                probably_guarantee INTEGER NOT NULL DEFAULT 0
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("CREATE TABLE db_version (version INTEGER NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO db_version (version) VALUES (5)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO users (user_id, username, first_name, last_name, count, last_time, is_admin, is_banned, probably_guarantee)
             VALUES (1, 'alice', 'Alice', 'A', 10, 1700000000, 1, 0, 5),
                    (2, '', 'Bob', NULL, 3, 1700000001, 0, 2, 42)",
        )
        .execute(&pool)
        .await
        .unwrap();

        init_database(&pool, DatabaseKind::Sqlite).await;

        assert_eq!(db_version(&pool).await, CURRENT_DB_VERSION);
        assert_eq!(username_of(&pool, 2).await, None);
        // every other column must survive the table rebuild
        assert!(is_admin(&pool, DatabaseKind::Sqlite, 1).await.unwrap());
        assert_eq!(ban_status(&pool, DatabaseKind::Sqlite, 2).await.unwrap(), 2);
        assert_eq!(
            get_probably_guarantee(&pool, DatabaseKind::Sqlite, 2)
                .await
                .unwrap(),
            42
        );

        // a brand new user with no Telegram username must now be storable
        upsert_user(
            &pool,
            DatabaseKind::Sqlite,
            &UserIdent {
                user_id: 3,
                username: None,
                first_name: Some("NoName"),
                last_name: None,
            },
            1,
            1700000002,
        )
        .await
        .expect("inserting a user without a username must succeed");

        let found = find_user_by_id_or_username(&pool, DatabaseKind::Sqlite, "3")
            .await
            .expect("reading a user without a username must not error")
            .expect("user 3 should exist");
        assert_eq!(found.2, None);
        assert_eq!(
            get_user_display_name(
                found.3.as_deref(),
                found.4.as_deref(),
                found.2.as_deref(),
                3
            ),
            "NoName"
        );

        let _ = std::fs::remove_file(path);
    }

    /// init_database runs on every startup; a second run must be a no-op.
    #[tokio::test]
    async fn migration_is_idempotent() {
        let (pool, path) = fresh_sqlite("idempotent").await;
        init_database(&pool, DatabaseKind::Sqlite).await;
        upsert_user(
            &pool,
            DatabaseKind::Sqlite,
            &UserIdent {
                user_id: 1,
                username: None,
                first_name: Some("Alice"),
                last_name: None,
            },
            9,
            1700000000,
        )
        .await
        .unwrap();

        init_database(&pool, DatabaseKind::Sqlite).await;
        init_database(&pool, DatabaseKind::Sqlite).await;

        assert_eq!(db_version(&pool).await, CURRENT_DB_VERSION);
        assert_eq!(username_of(&pool, 1).await, None);
        assert_eq!(
            get_user_count_and_last_time(&pool, DatabaseKind::Sqlite, 1)
                .await
                .unwrap(),
            (9, Some(1700000000))
        );

        let _ = std::fs::remove_file(path);
    }

    /// sync_user_info must not resurrect the empty-string placeholder.
    #[tokio::test]
    async fn sync_user_info_stores_null_for_missing_username() {
        let (pool, path) = fresh_sqlite("sync-null").await;
        init_database(&pool, DatabaseKind::Sqlite).await;

        sync_user_info(&pool, DatabaseKind::Sqlite, 1, None, Some("Alice"), None)
            .await
            .unwrap();
        assert_eq!(username_of(&pool, 1).await, None);

        // gaining a username, then losing it again
        sync_user_info(
            &pool,
            DatabaseKind::Sqlite,
            1,
            Some("alice"),
            Some("Alice"),
            None,
        )
        .await
        .unwrap();
        assert_eq!(username_of(&pool, 1).await.as_deref(), Some("alice"));

        sync_user_info(&pool, DatabaseKind::Sqlite, 1, None, Some("Alice"), None)
            .await
            .unwrap();
        assert_eq!(username_of(&pool, 1).await, None);

        let _ = std::fs::remove_file(path);
    }

    /// The rebuild commits in its own transaction, but db_version is bumped afterwards.
    /// A crash in between must leave the v5 -> v6 step safely re-runnable on the next start.
    #[tokio::test]
    async fn interrupted_migration_reruns_cleanly() {
        let (pool, path) = fresh_sqlite("interrupted").await;
        init_database(&pool, DatabaseKind::Sqlite).await;
        upsert_user(
            &pool,
            DatabaseKind::Sqlite,
            &UserIdent {
                user_id: 1,
                username: Some("alice"),
                first_name: Some("Alice"),
                last_name: None,
            },
            9,
            1700000000,
        )
        .await
        .unwrap();

        // simulate: table was already rebuilt, then the process died before the version bump
        sqlx::query("UPDATE db_version SET version = 5")
            .execute(&pool)
            .await
            .unwrap();

        init_database(&pool, DatabaseKind::Sqlite).await;

        assert_eq!(db_version(&pool).await, CURRENT_DB_VERSION);
        assert_eq!(username_of(&pool, 1).await.as_deref(), Some("alice"));
        assert_eq!(
            get_user_count_and_last_time(&pool, DatabaseKind::Sqlite, 1)
                .await
                .unwrap(),
            (9, Some(1700000000))
        );

        let _ = std::fs::remove_file(path);
    }
}
