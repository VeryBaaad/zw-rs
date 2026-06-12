/*
 * Copyright (C) 2026 VeryBaaad <verybaaad@outlook.com>
 * SPDX-License-Identifier: MIT
 */
use crate::i18n::{Locale, get_translation};
use crate::services::{handle_rank, handle_zw};
use crate::utils::DbPool;
use crate::utils::config::DatabaseKind;
use crate::utils::db::{
    ban_status, delete_user, is_admin, set_user_count, set_user_last_time, sync_user_info,
};
use crate::utils::fun::eunjeong_generate;
use crate::utils::logger::log;
use log::Level;
use rand::RngExt;
use rand::rng;
use std::error::Error;
use teloxide::types::ReplyParameters;
use teloxide::{prelude::*, utils::command::BotCommands};
use tokio::time::{Duration, sleep};

#[derive(BotCommands, Clone, Debug)]
#[command(rename_rule = "lowercase")]
pub enum Command {
    Zw(String),
    Rank(String),
    Version,
    Eunjeong(String),
    Set(String),
    Reset(String),
    Continue(String),
}

pub async fn commands_handler(
    bot: Bot,
    msg: Message,
    cmd: Command,
    pool: DbPool,
    database_kind: DatabaseKind,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    log(
        Level::Debug,
        "i18n",
        &format!(
            "user {} language_code: {:?}",
            msg.from.as_ref().map_or(0, |u| u.id.0),
            msg.from.as_ref().and_then(|u| u.language_code.as_deref())
        ),
    );
    let locale =
        Locale::from_language_code(msg.from.as_ref().and_then(|u| u.language_code.as_deref()));
    let t = get_translation(locale);

    log(
        Level::Info,
        "commands_handler",
        &format!("Received command: {:?}", cmd),
    );
    if ban_status(
        &pool,
        database_kind,
        msg.from.as_ref().map_or(0, |u| u.id.0 as i64),
    )
    .await?
        == 1
    {
        log(
            Level::Info,
            "commands_handler",
            &format!(
                "User {} is banned and attempted to use command: {:?}",
                msg.from.as_ref().map_or(0, |u| u.id.0),
                cmd
            ),
        );
        bot.send_message(msg.chat.id, t.banned_message())
            .reply_parameters(ReplyParameters::new(msg.id))
            .await?;
        return Ok(());
    }
    if ban_status(
        &pool,
        database_kind,
        msg.from.as_ref().map_or(0, |u| u.id.0 as i64),
    )
    .await?
        == 2
    {
        let millis: u64 = rng().random_range(3000..=10000);
        sleep(Duration::from_millis(millis)).await;
    }

    // Sync user info from Telegram to keep it up to date
    if let Some(user) = msg.from.as_ref() {
        let user_id = user.id.0 as i64;
        let username = user.username.as_deref();
        let first_name = Some(user.first_name.as_str());
        let last_name = user.last_name.as_deref();
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
                "commands_handler",
                &format!("Failed to sync user info for {}: {}", user_id, e),
            );
        }
    }

    match cmd {
        Command::Zw(arg) => {
            let arg = arg.trim();
            if arg.is_empty() {
                log(
                    Level::Debug,
                    "commands_handler",
                    "No argument provided for zw command, treating as empty",
                );
                handle_zw(bot, msg, pool, database_kind, locale, None).await?
            } else {
                log(
                    Level::Debug,
                    "commands_handler",
                    &format!("Received argument for zw command: '{}'", arg),
                );
                handle_zw(bot, msg, pool, database_kind, locale, Some(arg.to_string())).await?
            }
        }
        Command::Rank(arg) => {
            let page = if arg.is_empty() {
                0
            } else {
                arg.trim()
                    .parse::<usize>()
                    .ok()
                    .map(|p| if p > 0 { p - 1 } else { 0 })
                    .unwrap_or(0)
            };
            log(
                Level::Debug,
                "commands_handler",
                &format!("Parsed rank page argument: {}", page),
            );
            handle_rank(
                bot,
                msg.chat.id,
                None,
                Some(msg.id),
                pool,
                database_kind,
                locale,
                page,
            )
            .await?;
        }
        Command::Version => {
            let version_info = get_version_info().await?;
            bot.send_message(msg.chat.id, version_info).await?;
        }
        Command::Eunjeong(arg) => {
            let eunjeong_text = if arg.is_empty() {
                eunjeong_generate(None).await
            } else {
                let custom_count = arg.trim().parse::<usize>().ok();
                if custom_count <= Some(100) {
                    eunjeong_generate(custom_count).await
                } else {
                    eunjeong_generate(None).await
                }
            };
            bot.send_message(msg.chat.id, eunjeong_text).await?;
        }
        Command::Set(arg) => {
            if let Some(user) = msg.from {
                if !is_admin(&pool, database_kind, user.id.0 as i64)
                    .await
                    .unwrap_or(false)
                {
                    bot.send_message(msg.chat.id, t.permission_denied()).await?;
                    return Ok(());
                }
                let parts: Vec<&str> = arg.split_whitespace().collect();
                if parts.len() != 2 {
                    bot.send_message(msg.chat.id, t.set_usage()).await?;
                    return Ok(());
                }
                let target_id: i64 = match parts[0].parse() {
                    Ok(id) => id,
                    Err(_) => {
                        bot.send_message(msg.chat.id, t.invalid_user_id()).await?;
                        return Ok(());
                    }
                };
                let count: i64 = match parts[1].parse() {
                    Ok(c) => c,
                    Err(_) => {
                        bot.send_message(msg.chat.id, t.invalid_count()).await?;
                        return Ok(());
                    }
                };
                set_user_count(&pool, database_kind, target_id, count).await?;
                bot.send_message(msg.chat.id, t.user_count_set(target_id, count))
                    .await?;
            }
        }
        Command::Reset(arg) => {
            if let Some(user) = msg.from {
                let user_id = user.id.0 as i64;
                let admin_result = is_admin(&pool, database_kind, user_id).await;
                log(
                    Level::Debug,
                    "commands_handler",
                    &format!(
                        "Reset command: user_id={}, is_admin result={:?}",
                        user_id, admin_result
                    ),
                );
                if !admin_result.unwrap_or(false) {
                    bot.send_message(msg.chat.id, t.permission_denied()).await?;
                    return Ok(());
                }
                let target_id: i64 = match arg.trim().parse() {
                    Ok(id) => id,
                    Err(_) => {
                        bot.send_message(msg.chat.id, t.reset_usage()).await?;
                        return Ok(());
                    }
                };
                delete_user(&pool, database_kind, target_id).await?;
                bot.send_message(msg.chat.id, t.user_removed(target_id))
                    .await?;
            }
        }
        Command::Continue(arg) => {
            if let Some(user) = msg.from {
                let user_id = user.id.0 as i64;
                if !is_admin(&pool, database_kind, user_id)
                    .await
                    .unwrap_or(false)
                {
                    bot.send_message(msg.chat.id, t.permission_denied()).await?;
                    return Ok(());
                }
                let target_id: i64 = match arg.trim().parse() {
                    Ok(id) => id,
                    Err(_) => {
                        bot.send_message(msg.chat.id, t.continue_usage()).await?;
                        return Ok(());
                    }
                };
                set_user_last_time(&pool, database_kind, target_id).await?;
                bot.send_message(msg.chat.id, t.user_last_time_set(target_id))
                    .await?;
            }
        }
    }
    Ok(())
}

pub async fn get_version_info() -> Result<String, Box<dyn Error + Send + Sync>> {
    Ok(format!(
        "{} v{} ({})\n\
Commit {}\n\
Built at {}\n\
Target {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        env!("VER_CODE"),
        env!("GIT_HASH"),
        env!("BUILD_TIME"),
        env!("BUILD_TARGET")
    ))
}
