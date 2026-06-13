/*
 * Copyright (C) 2026 VeryBaaad <verybaaad@outlook.com>
 * SPDX-License-Identifier: MIT
 */
use crate::utils::DbPool;
use crate::utils::config::DatabaseKind;
use crate::utils::logger::log;
use crate::utils::{
    UserIdent, check_cooldown, find_user_by_id_or_username, format_user_mention,
    get_probably_guarantee, get_rank, get_user_count_and_last_time, set_probably_guarantee,
    sync_user_info, upsert_user,
};
use chrono::Duration;
use log::Level;
use rand::RngExt;
use rand::rng;
use std::error::Error;
use teloxide::{prelude::*, types::ReplyParameters, utils::markdown};

pub async fn handle_zw(
    bot: Bot,
    msg: Message,
    pool: DbPool,
    database_kind: DatabaseKind,
    target_arg: Option<String>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let user = msg.from.as_ref().unwrap();
    let initiator_id = user.id.0 as i64;
    let initiator_username = user.username.as_deref();
    let initiator_first_name = Some(user.first_name.as_str());
    let initiator_last_name = user.last_name.as_deref();

    // Sync user info from Telegram to database (keep it up to date)
    if let Err(e) = sync_user_info(
        &pool,
        database_kind,
        initiator_id,
        initiator_username,
        initiator_first_name,
        initiator_last_name,
    )
    .await
    {
        log(
            Level::Warn,
            "handle_zw",
            &format!("Failed to sync user info for {}: {}", initiator_id, e),
        );
    }

    let now = chrono::Utc::now().timestamp();
    let cd_duration = Duration::minutes(30);

    if target_arg.is_none() {
        return handle_zw_self(bot, msg, pool, database_kind).await;
    }

    let target_key = target_arg.unwrap();
    let target_key = target_key.trim().trim_start_matches('@').to_string();

    let target_record = find_user_by_id_or_username(&pool, database_kind, &target_key).await?;
    if target_record.is_none() {
        let text = format!("未找到用户 {} 的记录，无法进行帮助。", target_key);
        let _ = bot
            .send_message(msg.chat.id, text)
            .reply_parameters(ReplyParameters::new(msg.id))
            .await;
        return Ok(());
    }
    let (
        target_count,
        target_last_time_opt,
        target_username,
        target_first_name,
        target_last_name,
        target_user_id,
    ) = target_record.unwrap();
    let initiator_mention = format_user_mention(
        initiator_id,
        initiator_first_name,
        initiator_last_name,
        initiator_username,
    );
    let target_mention = format_user_mention(
        target_user_id,
        target_first_name.as_deref(),
        target_last_name.as_deref(),
        Some(&target_username),
    );

    let (initiator_count, initiator_last_time_opt) =
        get_user_count_and_last_time(&pool, database_kind, initiator_id).await?;

    let mut any_in_cd = false;
    let mut cd_messages = Vec::new();

    let initiator_cd = check_cooldown(initiator_last_time_opt, now, cd_duration);
    if initiator_cd.is_in_cooldown {
        any_in_cd = true;
        cd_messages.push(format!(
            "发起者 {} 仍在冷却：{}分{}秒",
            initiator_mention, initiator_cd.mins, initiator_cd.secs
        ));
    }

    let target_cd = check_cooldown(target_last_time_opt, now, cd_duration);
    if target_cd.is_in_cooldown {
        any_in_cd = true;
        cd_messages.push(format!(
            "另一位 {} 仍在冷却：{}分{}秒",
            target_mention, target_cd.mins, target_cd.secs
        ));
    }

    if any_in_cd {
        let initiator_rank = get_rank(&pool, initiator_id, database_kind)
            .await
            .unwrap_or(0);
        let target_rank = get_rank(&pool, target_user_id, database_kind)
            .await
            .unwrap_or(0);
        let text = format!(
            "{}，杂鱼杂鱼，他好像昏厥了呢\n\n\
发起者：{}\n\
次数：{}次\n\
排行榜位置：{}\n\n\
另一位：{}\n\
次数：{}次\n\
排行榜位置：{}\n\n\
{}",
            initiator_mention,
            initiator_mention,
            markdown::escape(initiator_count.to_string().as_str()),
            initiator_rank,
            target_mention,
            markdown::escape(target_count.to_string().as_str()),
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

    let initiator_probably_guarantee =
        get_probably_guarantee(&pool, database_kind, initiator_id).await?;
    let target_probably_guarantee =
        get_probably_guarantee(&pool, database_kind, target_user_id).await?;
    let average_probably_guarantee = (initiator_probably_guarantee + target_probably_guarantee) / 2;
    let probable_event: i64 = rng().random_range(1..=100);
    let (new_initiator_count, new_target_count, newer_time) = match probable_event {
        r if r <= (10 + average_probably_guarantee) => {
            set_probably_guarantee(&pool, database_kind, initiator_id, 0).await?;
            set_probably_guarantee(&pool, database_kind, target_user_id, 0).await?;
            (initiator_count + 50, target_count + 25, now + 1800)
        }
        _ => {
            let new_initiator_probably_guarantee = (initiator_probably_guarantee + 5).min(100);
            let new_target_probably_guarantee = (target_probably_guarantee + 5).min(100);
            set_probably_guarantee(
                &pool,
                database_kind,
                initiator_id,
                new_initiator_probably_guarantee,
            )
            .await?;
            set_probably_guarantee(
                &pool,
                database_kind,
                target_user_id,
                new_target_probably_guarantee,
            )
            .await?;
            (initiator_count + 1, target_count + 1, now)
        }
    };

    let mut tx = pool.begin().await?;
    upsert_user(
        &mut *tx,
        database_kind,
        &UserIdent {
            user_id: initiator_id,
            username: initiator_username,
            first_name: initiator_first_name,
            last_name: initiator_last_name,
        },
        new_initiator_count,
        newer_time,
    )
    .await?;
    upsert_user(
        &mut *tx,
        database_kind,
        &UserIdent {
            user_id: target_user_id,
            username: Some(&target_username),
            first_name: target_first_name.as_deref(),
            last_name: target_last_name.as_deref(),
        },
        new_target_count,
        newer_time,
    )
    .await?;
    tx.commit().await?;

    let initiator_rank = get_rank(&pool, initiator_id, database_kind).await?;
    let target_rank = get_rank(&pool, target_user_id, database_kind).await?;

    let text = match probable_event {
        r if r <= (10 + average_probably_guarantee) => {
            format!(
                "已进行调教！\n\n\
{} 陪 {} van游戏！\n\n\
发起者：{}次\n\
另一位：{}次\n\n\
您在自慰排行榜上的位置：{}\n\
另一位在自慰排行榜上的位置：{}\n\
下次可进行自慰的时间：60分0秒",
                initiator_mention,
                target_mention,
                markdown::escape(new_initiator_count.to_string().as_str()),
                markdown::escape(new_target_count.to_string().as_str()),
                initiator_rank,
                target_rank
            )
        }
        _ => {
            format!(
                "已进行双人运动！\n\n\
{} 带上 {} 进行了性行为！\n\n\
发起者：{}次\n\
另一位：{}次\n\n\
您在自慰排行榜上的位置：{}\n\
另一位在自慰排行榜上的位置：{}\n\
下次可进行自慰的时间：30分0秒",
                initiator_mention,
                target_mention,
                markdown::escape(new_initiator_count.to_string().as_str()),
                markdown::escape(new_target_count.to_string().as_str()),
                initiator_rank,
                target_rank
            )
        }
    };

    bot.send_message(msg.chat.id, &text)
        .reply_parameters(ReplyParameters::new(msg.id))
        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
        .await?;
    if !any_in_cd {
        bot.send_message(UserId(target_user_id as u64), text)
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .await?;
    }

    Ok(())
}

pub async fn handle_zw_self(
    bot: Bot,
    msg: Message,
    pool: DbPool,
    database_kind: DatabaseKind,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    log(Level::Debug, "handle_zw", "Handling zw command");
    let user = msg.from.as_ref().unwrap();
    let user_id = user.id.0 as i64;
    let username = user.username.as_deref();
    let first_name = Some(user.first_name.as_str());
    let last_name = user.last_name.as_deref();

    log(
        Level::Info,
        "handle_zw",
        &format!(
            "User: {} (ID: {}, Username: {:?})",
            user.first_name, user_id, username
        ),
    );

    // Sync user info
    if let Err(e) = sync_user_info(
        &pool,
        database_kind,
        user_id,
        username,
        first_name,
        last_name,
    )
    .await
    {
        log(
            Level::Warn,
            "handle_zw",
            &format!("Failed to sync user info for {}: {}", user_id, e),
        );
    }

    let user_mention = format_user_mention(user_id, first_name, last_name, username);

    let now = chrono::Utc::now().timestamp();
    let cd_duration = Duration::minutes(30);

    let (current_count, last_time) =
        get_user_count_and_last_time(&pool, database_kind, user_id).await?;

    let cd_status = check_cooldown(last_time, now, cd_duration);
    if cd_status.is_in_cooldown {
        log(
            Level::Warn,
            "handle_zw",
            &format!("User {} still in cooldown", user_id),
        );
        let rank = get_rank(&pool, user_id, database_kind).await?;
        let text = format!(
            "{}，杂鱼杂鱼，已经达到顶峰了呢\\~\n\n\
您在自慰排行榜上的位置：{}\n\
总次数：{}次\n\
下次可进行自慰的时间：{}分{}秒",
            user_mention, rank, current_count, cd_status.mins, cd_status.secs
        );
        if let Err(e) = bot
            .send_message(msg.chat.id, text)
            .reply_parameters(ReplyParameters::new(msg.id))
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
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

    // Update count and last_time
    let current_probably_guarantee = get_probably_guarantee(&pool, database_kind, user_id).await?;
    let probable_event: i64 = rng().random_range(1..=100);
    let (new_count, newer_time) = match probable_event {
        r if r <= (10 + current_probably_guarantee) => {
            set_probably_guarantee(&pool, database_kind, user_id, 0).await?;
            (current_count + 25, now + 1800)
        }
        _ => {
            let new_probably_guarantee = (current_probably_guarantee + 5).min(100);
            set_probably_guarantee(&pool, database_kind, user_id, new_probably_guarantee).await?;
            (current_count + 1, now)
        }
    };
    log(
        Level::Info,
        "handle_zw",
        &format!("Updating user count: {} -> {}", current_count, new_count),
    );
    upsert_user(
        &pool,
        database_kind,
        &UserIdent {
            user_id,
            username,
            first_name,
            last_name,
        },
        new_count,
        newer_time,
    )
    .await?;

    let rank = get_rank(&pool, user_id, database_kind).await?;
    let text = match probable_event {
        r if r <= (10 + current_probably_guarantee) => {
            format!(
                "到顶了呢♡\n\n\
您在自慰排行榜上的位置：{}\n\
总次数：{}次\n\
下次可进行自慰的时间：60分0秒",
                rank, new_count
            )
        }
        _ => {
            format!(
                "已开始自慰！\n\n\
您在自慰排行榜上的位置：{}\n\
总次数：{}次\n\
下次可进行自慰的时间：30分0秒",
                rank, new_count
            )
        }
    };
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
    pool: &DbPool,
    database_kind: DatabaseKind,
    user_id: i64,
    username: Option<&str>,
    first_name: Option<&str>,
    last_name: Option<&str>,
) -> Result<(String, i64), Box<dyn Error + Send + Sync>> {
    let user_mention = format_user_mention(user_id, first_name, last_name, username);
    log(
        Level::Debug,
        "process_zw_for_user",
        &format!("Processing zw for user {} ({})", user_mention, user_id),
    );
    let now = chrono::Utc::now().timestamp();
    let cd_duration = Duration::minutes(30);

    let (current_count, last_time_opt) =
        get_user_count_and_last_time(pool, database_kind, user_id).await?;

    // CD Check
    let cd_status = check_cooldown(last_time_opt, now, cd_duration);
    if cd_status.is_in_cooldown {
        let rank = get_rank(pool, user_id, database_kind).await.unwrap_or(0);
        let text = format!(
            "{}，杂鱼杂鱼，已经达到顶峰了呢\\~\n\n\
您在自慰排行榜上的位置：{}\n\
总次数：{}次\n\
下次可进行自慰的时间：{}分{}秒",
            user_mention, rank, current_count, cd_status.mins, cd_status.secs
        );
        return Ok((text, current_count));
    }

    // Update count and last_time
    let current_probably_guarantee = get_probably_guarantee(pool, database_kind, user_id).await?;
    let probable_event: i64 = rng().random_range(1..=100);
    let (new_count, newer_time) = match probable_event {
        r if r <= (10 + current_probably_guarantee) => {
            set_probably_guarantee(pool, database_kind, user_id, 0).await?;
            (current_count + 25, now + 1800)
        }
        _ => {
            let new_probably_guarantee = (current_probably_guarantee + 5).min(100);
            set_probably_guarantee(pool, database_kind, user_id, new_probably_guarantee).await?;
            (current_count + 1, now)
        }
    };
    upsert_user(
        pool,
        database_kind,
        &UserIdent {
            user_id,
            username,
            first_name,
            last_name,
        },
        new_count,
        newer_time,
    )
    .await?;

    let rank = get_rank(pool, user_id, database_kind).await?;
    let text = match probable_event {
        r if r <= (10 + current_probably_guarantee) => {
            format!(
                "到顶了呢♡\n\n\
您在自慰排行榜上的位置：{}\n\
总次数：{}次\n\
下次可进行自慰的时间：60分0秒",
                rank, new_count
            )
        }
        _ => {
            format!(
                "已开始自慰！\n\n\
您在自慰排行榜上的位置：{}\n\
总次数：{}次\n\
下次可进行自慰的时间：30分0秒",
                rank, new_count
            )
        }
    };
    Ok((text, new_count))
}

pub async fn process_zw_help_for_user(
    pool: &DbPool,
    database_kind: DatabaseKind,
    initiator: &UserIdent<'_>,
    target: &UserIdent<'_>,
) -> Result<(String, bool), Box<dyn Error + Send + Sync>> {
    let initiator_mention = format_user_mention(
        initiator.user_id,
        initiator.first_name,
        initiator.last_name,
        initiator.username,
    );
    let target_mention = format_user_mention(
        target.user_id,
        target.first_name,
        target.last_name,
        target.username,
    );
    log(
        Level::Debug,
        "process_zw_help_for_user",
        &format!(
            "Processing zw help: {} helping {}",
            initiator_mention, target_mention
        ),
    );
    let now = chrono::Utc::now().timestamp();
    let cd_duration = Duration::minutes(30);

    let (initiator_count, initiator_last_time_opt) =
        get_user_count_and_last_time(pool, database_kind, initiator.user_id).await?;
    let (target_count, target_last_time_opt) =
        get_user_count_and_last_time(pool, database_kind, target.user_id).await?;

    // CD Check
    let mut any_in_cd = false;
    let mut cd_messages = Vec::new();

    if let Some(lt) = initiator_last_time_opt {
        let cd_status = check_cooldown(Some(lt), now, cd_duration);
        if cd_status.is_in_cooldown {
            any_in_cd = true;
            cd_messages.push(format!(
                "发起者 {} 仍在冷却：{}分{}秒",
                initiator_mention, cd_status.mins, cd_status.secs
            ));
        }
    }
    if let Some(lt) = target_last_time_opt {
        let cd_status = check_cooldown(Some(lt), now, cd_duration);
        if cd_status.is_in_cooldown {
            any_in_cd = true;
            cd_messages.push(format!(
                "另一位 {} 仍在冷却：{}分{}秒",
                target_mention, cd_status.mins, cd_status.secs
            ));
        }
    }

    if any_in_cd {
        let initiator_rank = get_rank(pool, initiator.user_id, database_kind)
            .await
            .unwrap_or(0);
        let target_rank = get_rank(pool, target.user_id, database_kind)
            .await
            .unwrap_or(0);
        return Ok((
            format!(
                "{}，杂鱼杂鱼，他好像昏厥了呢\n\n\
发起者：{}\n\
次数：{}次\n\
排行榜位置：{}\n\n\
另一位：{}\n\
次数：{}次\n\
排行榜位置：{}\n\n\
{}",
                initiator_mention,
                initiator_mention,
                markdown::escape(initiator_count.to_string().as_str()),
                initiator_rank,
                target_mention,
                markdown::escape(target_count.to_string().as_str()),
                target_rank,
                cd_messages.join("\n")
            ),
            false,
        ));
    }

    // Update both users
    let initiator_probably_guarantee =
        get_probably_guarantee(pool, database_kind, initiator.user_id).await?;
    let target_probably_guarantee =
        get_probably_guarantee(pool, database_kind, target.user_id).await?;
    let average_probably_guarantee = (initiator_probably_guarantee + target_probably_guarantee) / 2;
    let probable_event: i64 = rng().random_range(1..=100);
    let (new_initiator_count, new_target_count, newer_time) = match probable_event {
        r if r <= (10 + average_probably_guarantee) => {
            set_probably_guarantee(pool, database_kind, initiator.user_id, 0).await?;
            set_probably_guarantee(pool, database_kind, target.user_id, 0).await?;
            (initiator_count + 50, target_count + 25, now + 1800)
        }
        _ => {
            let new_initiator_probably_guarantee = (initiator_probably_guarantee + 5).min(100);
            let new_target_probably_guarantee = (target_probably_guarantee + 5).min(100);
            set_probably_guarantee(
                pool,
                database_kind,
                initiator.user_id,
                new_initiator_probably_guarantee,
            )
            .await?;
            set_probably_guarantee(
                pool,
                database_kind,
                target.user_id,
                new_target_probably_guarantee,
            )
            .await?;
            (initiator_count + 1, target_count + 1, now)
        }
    };

    let mut tx = pool.begin().await?;
    upsert_user(
        &mut *tx,
        database_kind,
        initiator,
        new_initiator_count,
        newer_time,
    )
    .await?;
    upsert_user(
        &mut *tx,
        database_kind,
        target,
        new_target_count,
        newer_time,
    )
    .await?;

    tx.commit().await?;

    let initiator_rank = get_rank(pool, initiator.user_id, database_kind).await?;
    let target_rank = get_rank(pool, target.user_id, database_kind).await?;

    let text = match probable_event {
        r if r <= (10 + average_probably_guarantee) => {
            format!(
                "已进行调教！\n\n\
{} 陪 {} van游戏！\n\n\
发起者：{}次\n\
另一位：{}次\n\n\
您在自慰排行榜上的位置：{}\n\
另一位在自慰排行榜上的位置：{}\n\
下次可进行自慰的时间：60分0秒",
                initiator_mention,
                target_mention,
                markdown::escape(new_initiator_count.to_string().as_str()),
                markdown::escape(new_target_count.to_string().as_str()),
                initiator_rank,
                target_rank
            )
        }
        _ => {
            format!(
                "已进行双人运动！\n\n\
{} 带上 {} 进行了性行为！\n\n\
发起者：{}次\n\
另一位：{}次\n\n\
您在自慰排行榜上的位置：{}\n\
另一位在自慰排行榜上的位置：{}\n\
下次可进行自慰的时间：30分0秒",
                initiator_mention,
                target_mention,
                markdown::escape(new_initiator_count.to_string().as_str()),
                markdown::escape(new_target_count.to_string().as_str()),
                initiator_rank,
                target_rank
            )
        }
    };

    Ok((text, true))
}
