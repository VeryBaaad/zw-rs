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

const CURRENT_DB_VERSION: i32 = 4;

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
                username TEXT NOT NULL,
                first_name TEXT,
                last_name TEXT,
                count INTEGER NOT NULL DEFAULT 0,
                last_time BIGINT NOT NULL DEFAULT 0,
                is_admin BOOLEAN NOT NULL DEFAULT 0,
                is_banned INTEGER NOT NULL DEFAULT 0
            )"
        }
        DatabaseKind::Postgres => {
            "CREATE TABLE IF NOT EXISTS users (
                user_id BIGINT NOT NULL UNIQUE,
                username TEXT NOT NULL,
                first_name TEXT,
                last_name TEXT,
                count BIGINT NOT NULL DEFAULT 0,
                last_time BIGINT NOT NULL DEFAULT 0,
                is_admin BOOLEAN NOT NULL DEFAULT FALSE,
                is_banned INTEGER NOT NULL DEFAULT 0
            )"
        }
        DatabaseKind::MySql | DatabaseKind::MariaDb => {
            "CREATE TABLE IF NOT EXISTS users (
                user_id BIGINT NOT NULL UNIQUE,
                username TEXT NOT NULL,
                first_name TEXT,
                last_name TEXT,
                count BIGINT NOT NULL DEFAULT 0,
                last_time BIGINT NOT NULL DEFAULT 0,
                is_admin BOOLEAN NOT NULL DEFAULT FALSE,
                is_banned INTEGER NOT NULL DEFAULT 0
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

    sqlx::query(
        "CREATE TABLE users (
            user_id INTEGER NOT NULL UNIQUE,
            username TEXT NOT NULL,
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
    user_id: i64,
    username: Option<&str>,
    first_name: Option<&str>,
    last_name: Option<&str>,
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
        .bind(user_id)
        .bind(username)
        .bind(first_name)
        .bind(last_name)
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
        String,
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
            let username: String = row.try_get("username")?;
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
    if let Some(row) = sqlx::query(sql_by_name)
        .bind(uname)
        .fetch_optional(pool)
        .await?
    {
        let count: i64 = row.try_get("count")?;
        let last_time: Option<i64> = row.try_get("last_time").ok();
        let username: String = row.try_get("username")?;
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
        let stored_username: String = row.try_get("username").unwrap_or_default();
        let stored_first_name: Option<String> = row.try_get("first_name").ok();
        let stored_last_name: Option<String> = row.try_get("last_name").ok();

        let new_username = username.unwrap_or("");
        let username_changed = stored_username != new_username;
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
            user_id,
            username,
            first_name,
            last_name,
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
