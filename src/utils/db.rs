/*
 * Copyright (C) 2026 VeryBaaad <verybaaad@outlook.com>
 * SPDX-License-Identifier: MIT
 */
use crate::utils::logger::log;
use chrono::{Duration, Utc};
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

pub async fn user_exists(
    pool: &SqlitePool,
    user_id: i64,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
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

/// Get user count and last_time by user_id
/// Returns (count, last_time)
pub async fn get_user_count_and_last_time(
    pool: &SqlitePool,
    user_id: i64,
) -> Result<(i64, Option<chrono::DateTime<Utc>>), Box<dyn Error + Send + Sync>> {
    log(
        Level::Debug,
        "get_user_count_and_last_time",
        &format!("Fetching count and last_time for user {}", user_id),
    );
    let row = sqlx::query("SELECT count, last_time FROM users WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

    if let Some(row) = row {
        let count: i64 = row.try_get("count")?;
        let last_time: Option<chrono::DateTime<Utc>> = row.try_get("last_time").ok();
        Ok((count, last_time))
    } else {
        Ok((0, None))
    }
}

/// Find user by ID or username, returns (count, last_time, username, user_id)
pub async fn find_user_by_id_or_username(
    pool: &SqlitePool,
    key: &str,
) -> Result<Option<(i64, Option<chrono::DateTime<Utc>>, String, i64)>, Box<dyn Error + Send + Sync>>
{
    log(
        Level::Debug,
        "find_user_by_id_or_username",
        &format!("Searching for user by key: {}", key),
    );

    // try to parse as user_id first
    if let Ok(id) = key.parse::<i64>() {
        if let Some(row) =
            sqlx::query("SELECT count, last_time, username, user_id FROM users WHERE user_id = ?")
                .bind(id)
                .fetch_optional(pool)
                .await?
        {
            let count: i64 = row.try_get("count")?;
            let last_time: Option<chrono::DateTime<Utc>> = row.try_get("last_time").ok();
            let username: String = row.try_get("username")?;
            let user_id: i64 = row.try_get("user_id")?;
            return Ok(Some((count, last_time, username, user_id)));
        }
        return Ok(None);
    }

    // try to parse as username (with optional @)
    let uname = key.trim_start_matches('@');
    if let Some(row) =
        sqlx::query("SELECT count, last_time, username, user_id FROM users WHERE username = ?")
            .bind(uname)
            .fetch_optional(pool)
            .await?
    {
        let count: i64 = row.try_get("count")?;
        let last_time: Option<chrono::DateTime<Utc>> = row.try_get("last_time").ok();
        let username: String = row.try_get("username")?;
        let user_id: i64 = row.try_get("user_id")?;
        Ok(Some((count, last_time, username, user_id)))
    } else {
        Ok(None)
    }
}

/// Cooldown status check result
#[derive(Debug, Clone)]
pub struct CooldownStatus {
    pub is_in_cooldown: bool,
    pub mins: i64,
    pub secs: i64,
}

/// Check cooldown status for a user
pub fn check_cooldown(
    last_time: Option<chrono::DateTime<Utc>>,
    now: chrono::DateTime<Utc>,
    duration: Duration,
) -> CooldownStatus {
    if let Some(lt) = last_time {
        let next_time = lt + duration;
        if now < next_time {
            let remaining = next_time - now;
            let mins = remaining.num_minutes();
            let secs = remaining.num_seconds() % 60;
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
