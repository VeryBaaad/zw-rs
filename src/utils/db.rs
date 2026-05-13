/*
 * Copyright (C) 2026 VeryBaaad <verybaaad@outlook.com>
 * SPDX-License-Identifier: MIT
 */
use crate::utils::logger::log;
use chrono::Utc;
use log::Level;
use sqlx::{Row, SqlitePool};
use std::error::Error;

pub async fn upsert_user<'a, E>(
    pool: E,
    user_id: i64,
    username: &str,
    new_count: i64,
    now: chrono::DateTime<Utc>,
) -> Result<(), sqlx::Error>
where
    E: sqlx::Executor<'a, Database = sqlx::Sqlite>,
{
    log(
        Level::Debug,
        "handle_zw",
        "Inserting/updating user in database",
    );
    if let Err(e) = sqlx::query(
        "INSERT INTO users (user_id, username, count, last_time) VALUES (?, ?, ?, ?)
         ON CONFLICT(user_id) DO UPDATE SET
         username = excluded.username,
         count = excluded.count,
         last_time = excluded.last_time",
    )
    .bind(user_id)
    .bind(username)
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

pub async fn user_exists(pool: &SqlitePool, user_id: i64) -> Result<bool, Box<dyn Error + Send + Sync>> {
    log(
        Level::Debug,
        "user_exists",
        &format!("Checking if user {} exists", user_id),
    );
    let row = sqlx::query("SELECT user_id FROM users WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

pub async fn get_total_users(pool: &SqlitePool) -> Result<i64, Box<dyn Error + Send + Sync>> {
    log(Level::Debug, "get_total_users", "Fetching total user count");
    let row = sqlx::query("SELECT COUNT(*) as count FROM users")
        .fetch_one(pool)
        .await?;
    let count: i64 = row.try_get("count")?;
    Ok(count)
}

pub async fn get_rank(
    pool: &SqlitePool,
    user_id: i64,
) -> Result<usize, Box<dyn Error + Send + Sync>> {
    log(
        Level::Debug,
        "get_rank",
        &format!("Calculating rank for user: {}", user_id),
    );
    let row = match sqlx::query(
        "SELECT COUNT(*) as rank FROM users WHERE count > (SELECT count FROM users WHERE user_id = ?) OR (count = (SELECT count FROM users WHERE user_id = ?) AND last_time < (SELECT last_time FROM users WHERE user_id = ?))"
    )
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .fetch_one(pool)
    .await {
        Ok(r) => r,
        Err(e) => {
            log(Level::Error, "get_rank", &format!("Failed to fetch rank for user {}: {}", user_id, e));
            return Err(Box::new(e));
        }
    };
    let rank: i64 = row.try_get("rank")?;
    let final_rank = (rank + 1) as usize;
    log(
        Level::Debug,
        "get_rank",
        &format!("User {} rank: {}", user_id, final_rank),
    );
    Ok(final_rank)
}
