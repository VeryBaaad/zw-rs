/*
 * Copyright (C) 2026 VeryBaaad <verybaaad@outlook.com>
 * SPDX-License-Identifier: MIT
 */
use crate::utils::logger::log;
use crate::utils::{get_rank, upsert_user};
use chrono::{Duration, Utc};
use log::Level;
use sqlx::{Row, SqlitePool};
use std::error::Error;
use teloxide::{
    prelude::*,
    types::ReplyParameters,
    utils::markdown,
};

pub async fn handle_zw(
    bot: Bot,
    msg: Message,
    pool: SqlitePool,
    target_arg: Option<String>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let user = msg.from.as_ref().unwrap();
    let initiator_id = user.id.0 as i64;
    let initiator_username = user.username.as_deref().unwrap_or("未知用户");
    let initiator_name = match user.last_name.as_deref() {
        Some(last_name) => format!("{} {}", user.first_name, last_name),
        None => user.first_name.clone(),
    };

    let now = Utc::now();
    let cd_duration = Duration::minutes(30);

    // find the target user record by id or username
    async fn find_user_record(
        pool: &SqlitePool,
        key: &str,
    ) -> Result<Option<(i64, Option<chrono::DateTime<Utc>>, String, i64)>, sqlx::Error> {
        // try to parse as user_id first
        if let Ok(id) = key.parse::<i64>() {
            if let Some(row) = sqlx::query(
                "SELECT count, last_time, username, user_id FROM users WHERE user_id = ?",
            )
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
            Ok(None)
        } else {
            // try to parse as username (with optional @)
            let uname = key.trim_start_matches('@');
            if let Some(row) = sqlx::query(
                "SELECT count, last_time, username, user_id FROM users WHERE username = ?",
            )
            .bind(uname)
            .fetch_optional(pool)
            .await?
            {
                let count: i64 = row.try_get("count")?;
                let last_time: Option<chrono::DateTime<Utc>> = row.try_get("last_time").ok();
                let username: String = row.try_get("username")?;
                let user_id: i64 = row.try_get("user_id")?;
                return Ok(Some((count, last_time, username, user_id)));
            }
            Ok(None)
        }
    }

    if target_arg.is_none() {
        return handle_zw_self(bot, msg, pool).await;
    }

    let target_key = target_arg.unwrap();
    let target_key = target_key.trim().trim_start_matches('@').to_string();

    let target_record = find_user_record(&pool, &target_key).await?;
    if target_record.is_none() {
        let text = format!("未找到用户 {} 的记录，无法进行帮助。", target_key);
        let _ = bot
            .send_message(msg.chat.id, text)
            .reply_parameters(ReplyParameters::new(msg.id))
            .await;
        return Ok(());
    }
    let (target_count, target_last_time_opt, target_username, target_user_id) =
        target_record.unwrap();

    let initiator_row = sqlx::query("SELECT count, last_time FROM users WHERE user_id = ?")
        .bind(initiator_id)
        .fetch_optional(&pool)
        .await?;
    let (initiator_count, initiator_last_time_opt) = if let Some(row) = initiator_row {
        let c: i64 = row.try_get("count")?;
        let lt: Option<chrono::DateTime<Utc>> = row.try_get("last_time").ok();
        (c, lt)
    } else {
        (0, None)
    };

    let mut any_in_cd = false;
    let mut cd_messages = Vec::new();

    if let Some(lt) = initiator_last_time_opt {
        let next = lt + cd_duration;
        if now < next {
            any_in_cd = true;
            let remaining = next - now;
            let mins = remaining.num_minutes();
            let secs = remaining.num_seconds() % 60;
            cd_messages.push(format!(
                "发起者 {} 仍在冷却：{}分{}秒",
                markdown::user_mention(UserId(initiator_id as u64), initiator_name.as_str()),
                mins,
                secs
            ));
        }
    }
    if let Some(lt) = target_last_time_opt {
        let next = lt + cd_duration;
        if now < next {
            any_in_cd = true;
            let remaining = next - now;
            let mins = remaining.num_minutes();
            let secs = remaining.num_seconds() % 60;
            cd_messages.push(format!(
                "另一位 {} 仍在冷却：{}分{}秒",
                markdown::user_mention(UserId(target_user_id as u64), target_username.as_str()),
                mins,
                secs
            ));
        }
    }

    if any_in_cd {
        let initiator_rank = get_rank(&pool, initiator_id).await.unwrap_or(0);
        let target_rank = get_rank(&pool, target_user_id).await.unwrap_or(0);
        let text = format!(
            "{}，杂鱼杂鱼，他好像昏厥了呢\n\n\
发起者：{}\n\
次数：{}次\n\
排行榜位置：{}\n\n\
另一位：{}\n\
次数：{}次\n\
排行榜位置：{}\n\n\
{}",
            initiator_name,
            markdown::user_mention(UserId(initiator_id as u64), initiator_name.as_str()),
            initiator_count,
            initiator_rank,
            markdown::user_mention(UserId(target_user_id as u64), target_username.as_str()),
            target_count,
            target_rank,
            cd_messages.join("\n")
        );
        let _ = bot
            .send_message(msg.chat.id, text)
            .reply_parameters(ReplyParameters::new(msg.id))
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .await;
        return Ok(());
    }

    let new_initiator_count = initiator_count + 1;
    let new_target_count = target_count + 1;

    let mut tx = pool.begin().await?;
    upsert_user(
        &mut *tx,
        initiator_id,
        initiator_username,
        new_initiator_count,
        now,
    )
    .await?;
    upsert_user(
        &mut *tx,
        target_user_id,
        &target_username,
        new_target_count,
        now,
    )
    .await?;
    tx.commit().await?;

    let initiator_rank = get_rank(&pool, initiator_id).await?;
    let target_rank = get_rank(&pool, target_user_id).await?;

    let text = format!(
        "已进行双人运动！\n\n\
{} 带上 {} 进行了性行为！\n\n\
发起者：{}次\n\
另一位：{}次\n\n\
您在自慰排行榜上的位置：{}\n\
另一位在自慰排行榜上的位置：{}\n\
下次可进行自慰的时间：30分0秒",
        markdown::user_mention(UserId(initiator_id as u64), initiator_name.as_str()),
        markdown::user_mention(UserId(target_user_id as u64), target_username.as_str()),
        new_initiator_count,
        new_target_count,
        initiator_rank,
        target_rank
    );

    bot.send_message(msg.chat.id, text)
        .reply_parameters(ReplyParameters::new(msg.id))
        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
        .await?;

    Ok(())
}

pub async fn handle_zw_self(
    bot: Bot,
    msg: Message,
    pool: SqlitePool,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    log(Level::Debug, "handle_zw", "Handling zw command");
    let user = msg.from.as_ref().unwrap();
    let user_id = user.id.0 as i64;
    let username = user.username.as_deref().unwrap_or("未知用户");
    let name = match user.last_name.as_deref() {
        Some(last_name) => format!("{} {}", user.first_name, last_name),
        None => user.first_name.clone(),
    };
    log(
        Level::Info,
        "handle_zw",
        &format!("User: {} (ID: {}, Username: {})", name, user_id, username),
    );

    let now = Utc::now();
    let cd_duration = Duration::minutes(30);

    // Check if user exists
    log(
        Level::Debug,
        "handle_zw",
        &format!("Querying user {} from database", user_id),
    );
    let row = sqlx::query("SELECT count, last_time FROM users WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(&pool)
        .await?;
    log(
        Level::Debug,
        "handle_zw",
        &format!("Query result: user_exists = {}", row.is_some()),
    );

    let (current_count, last_time) = if let Some(row) = row {
        let count: i64 = row.try_get("count")?;
        let last_time: chrono::DateTime<Utc> = row.try_get("last_time")?;
        log(
            Level::Debug,
            "handle_zw",
            &format!("User exists: count={}, last_time={}", count, last_time),
        );
        (count, Some(last_time))
    } else {
        log(
            Level::Debug,
            "handle_zw",
            "New user, count=0, last_time=None",
        );
        (0, None)
    };

    if let Some(last_time) = last_time {
        let next_time = last_time + cd_duration;
        log(
            Level::Debug,
            "handle_zw",
            &format!("Checking cooldown: now={}, next_time={}", now, next_time),
        );
        if now < next_time {
            log(
                Level::Warn,
                "handle_zw",
                &format!("User {} still in cooldown", user_id),
            );
            let remaining = next_time - now;
            let mins = remaining.num_minutes();
            let secs = remaining.num_seconds() % 60;
            log(
                Level::Debug,
                "handle_zw",
                &format!("Remaining cooldown: {}m{}s", mins, secs),
            );
            let rank = get_rank(&pool, user_id).await?;
            let text = format!(
                "{}，杂鱼杂鱼，已经达到顶峰了呢~\n\n\
您在自慰排行榜上的位置：{}\n\
总次数：{}次\n\
下次可进行自慰的时间：{}分{}秒",
                name, rank, current_count, mins, secs
            );
            if let Err(e) = bot
                .send_message(msg.chat.id, text)
                .reply_parameters(ReplyParameters::new(msg.id))
                .await
            {
                log(
                    Level::Error,
                    "handle_zw",
                    &format!("Failed to send cooldown message: {}", e),
                );
                return Err(Box::new(e));
            }
            return Ok(());
        }
        log(
            Level::Debug,
            "handle_zw",
            "Cooldown period expired, proceeding",
        );
    } else {
        log(
            Level::Debug,
            "handle_zw",
            "No previous record, first time user",
        );
    }

    // Update count and last_time
    let new_count = current_count + 1;
    log(
        Level::Info,
        "handle_zw",
        &format!("Updating user count: {} -> {}", current_count, new_count),
    );
    upsert_user(&pool, user_id, username, new_count, now).await?;

    let rank = get_rank(&pool, user_id).await?;
    let text = format!(
        "已开始自慰！\n\n\
您在自慰排行榜上的位置：{}\n\
总次数：{}次\n\
下次可进行自慰的时间：30分0秒",
        rank, new_count
    );
    if let Err(e) = bot
        .send_message(msg.chat.id, text)
        .reply_parameters(ReplyParameters::new(msg.id))
        .await
    {
        log(
            Level::Error,
            "handle_zw",
            &format!("Failed to send success message: {}", e),
        );
        return Err(Box::new(e));
    }
    log(
        Level::Info,
        "handle_zw",
        &format!(
            "User {} completed action, new count: {}",
            user_id, new_count
        ),
    );
    Ok(())
}

pub async fn process_zw_for_user(
    pool: &SqlitePool,
    user_id: i64,
    username: &str,
    display_name: &str,
) -> Result<(String, i64), Box<dyn Error + Send + Sync>> {
    log(
        Level::Debug,
        "process_zw_for_user",
        &format!("Processing zw for user {} ({})", display_name, user_id),
    );
    let now = Utc::now();
    let cd_duration = Duration::minutes(30);

    // Search for user record
    let row = sqlx::query("SELECT count, last_time FROM users WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    let (current_count, last_time_opt) = if let Some(row) = row {
        let count: i64 = row.try_get("count")?;
        let last_time: Option<chrono::DateTime<Utc>> = row.try_get("last_time").ok();
        (count, last_time)
    } else {
        (0, None)
    };

    // CD Check
    if let Some(last_time) = last_time_opt {
        let next_time = last_time + cd_duration;
        if now < next_time {
            let remaining = next_time - now;
            let mins = remaining.num_minutes();
            let secs = remaining.num_seconds() % 60;
            let rank = get_rank(pool, user_id).await.unwrap_or(0);
            let text = format!(
                "{}，杂鱼杂鱼，已经达到顶峰了呢~\n\n\
您在自慰排行榜上的位置：{}\n\
总次数：{}次\n\
下次可进行自慰的时间：{}分{}秒",
                display_name, rank, current_count, mins, secs
            );
            return Ok((text, current_count));
        }
    }

    // Update count and last_time
    let new_count = current_count + 1;
    upsert_user(pool, user_id, username, new_count, now).await?;

    let rank = get_rank(pool, user_id).await?;
    let text = format!(
        "已开始自慰！\n\n\
您在自慰排行榜上的位置：{}\n\
总次数：{}次\n\
下次可进行自慰的时间：30分0秒",
        rank, new_count
    );
    Ok((text, new_count))
}

pub async fn process_zw_help_for_user(
    pool: &SqlitePool,
    initiator_id: i64,
    initiator_username: &str,
    initiator_name: &str,
    target_id: i64,
    target_username: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    log(
        Level::Debug,
        "process_zw_help_for_user",
        &format!(
            "Processing zw help: {} helping {}",
            initiator_name, target_username
        ),
    );
    let now = Utc::now();
    let cd_duration = Duration::minutes(30);

    // Get initiator record
    let initiator_row = sqlx::query("SELECT count, last_time FROM users WHERE user_id = ?")
        .bind(initiator_id)
        .fetch_optional(pool)
        .await?;
    let (initiator_count, initiator_last_time_opt) = if let Some(row) = initiator_row {
        let c: i64 = row.try_get("count")?;
        let lt: Option<chrono::DateTime<Utc>> = row.try_get("last_time").ok();
        (c, lt)
    } else {
        (0, None)
    };

    // Get target record
    let target_row = sqlx::query("SELECT count, last_time FROM users WHERE user_id = ?")
        .bind(target_id)
        .fetch_optional(pool)
        .await?;
    let (target_count, target_last_time_opt) = if let Some(row) = target_row {
        let c: i64 = row.try_get("count")?;
        let lt: Option<chrono::DateTime<Utc>> = row.try_get("last_time").ok();
        (c, lt)
    } else {
        (0, None)
    };

    // CD Check
    let mut any_in_cd = false;
    let mut cd_messages = Vec::new();

    if let Some(lt) = initiator_last_time_opt {
        let next = lt + cd_duration;
        if now < next {
            any_in_cd = true;
            let remaining = next - now;
            let mins = remaining.num_minutes();
            let secs = remaining.num_seconds() % 60;
            cd_messages.push(format!(
                "发起者 {} 仍在冷却：{}分{}秒",
                markdown::user_mention(UserId(initiator_id as u64), initiator_name),
                mins,
                secs
            ));
        }
    }
    if let Some(lt) = target_last_time_opt {
        let next = lt + cd_duration;
        if now < next {
            any_in_cd = true;
            let remaining = next - now;
            let mins = remaining.num_minutes();
            let secs = remaining.num_seconds() % 60;
            cd_messages.push(format!(
                "另一位 {} 仍在冷却：{}分{}秒",
                markdown::user_mention(UserId(target_id as u64), target_username),
                mins,
                secs
            ));
        }
    }

    if any_in_cd {
        let initiator_rank = get_rank(pool, initiator_id).await.unwrap_or(0);
        let target_rank = get_rank(pool, target_id).await.unwrap_or(0);
        return Ok(format!(
            "{}，杂鱼杂鱼，他好像昏厥了呢\n\n\
发起者：{}\n\
次数：{}次\n\
排行榜位置：{}\n\n\
另一位：{}\n\
次数：{}次\n\
排行榜位置：{}\n\n\
{}",
            initiator_name,
            markdown::user_mention(UserId(initiator_id as u64), initiator_name),
            initiator_count,
            initiator_rank,
            markdown::user_mention(UserId(target_id as u64), target_username),
            target_count,
            target_rank,
            cd_messages.join("\n")
        ));
    }

    // Update both users
    let new_initiator_count = initiator_count + 1;
    let new_target_count = target_count + 1;

    let mut tx = pool.begin().await?;
    upsert_user(
        &mut *tx,
        initiator_id,
        initiator_username,
        new_initiator_count,
        now,
    )
    .await?;
    upsert_user(&mut *tx, target_id, target_username, new_target_count, now).await?;

    tx.commit().await?;

    let initiator_rank = get_rank(pool, initiator_id).await?;
    let target_rank = get_rank(pool, target_id).await?;

    let text = format!(
        "已进行双人运动！\n\n\
{} 带上 {} 进行了性行为！\n\n\
发起者：{}次\n\
另一位：{}次\n\n\
您在自慰排行榜上的位置：{}\n\
另一位在自慰排行榜上的位置：{}\n\
下次可进行自慰的时间：30分0秒",
        markdown::user_mention(UserId(initiator_id as u64), initiator_name),
        markdown::user_mention(UserId(target_id as u64), target_username),
        new_initiator_count,
        new_target_count,
        initiator_rank,
        target_rank
    );

    Ok(text)
}
