/*
 * Copyright (C) 2026 VeryBaaad <verybaaad@outlook.com>
 * SPDX-License-Identifier: MIT
 */
use crate::services::{handle_rank, handle_zw};
use crate::utils::db::{ban_status, delete_user, is_admin, set_user_count};
use crate::utils::logger::log;
use log::Level;
use rand::RngExt;
use rand::rng;
use sqlx::SqlitePool;
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
    Set(String),
    Reset(String),
}

pub async fn commands_handler(
    bot: Bot,
    msg: Message,
    cmd: Command,
    pool: SqlitePool,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    log(
        Level::Info,
        "commands_handler",
        &format!("Received command: {:?}", cmd),
    );
    if ban_status(&pool, msg.from.as_ref().map_or(0, |u| u.id.0 as i64)).await? == 1 {
        log(
            Level::Info,
            "commands_handler",
            &format!(
                "User {} is banned and attempted to use command: {:?}",
                msg.from.as_ref().map_or(0, |u| u.id.0),
                cmd
            ),
        );
        bot.send_message(
            msg.chat.id,
            "You have been permanently banned\n您已被永久封禁",
        )
        .reply_parameters(ReplyParameters::new(msg.id))
        .await?;
        return Ok(());
    }
    if ban_status(&pool, msg.from.as_ref().map_or(0, |u| u.id.0 as i64)).await? == 2 {
        let millis: u64 = rng().random_range(3000..=10000);
        sleep(Duration::from_millis(millis)).await;
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
                handle_zw(bot, msg, pool, None).await?
            } else {
                log(
                    Level::Debug,
                    "commands_handler",
                    &format!("Received argument for zw command: '{}'", arg),
                );
                handle_zw(bot, msg, pool, Some(arg.to_string())).await?
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
            handle_rank(bot, msg.chat.id, None, Some(msg.id), pool, page).await?;
        }
        Command::Version => {
            let version_info = get_version_info().await?;
            bot.send_message(msg.chat.id, version_info).await?;
        }
        Command::Set(arg) => {
            if let Some(user) = msg.from {
                if !is_admin(&pool, user.id.0 as i64).await.unwrap_or(false) {
                    bot.send_message(msg.chat.id, "Permission denied.").await?;
                    return Ok(());
                }
                let parts: Vec<&str> = arg.split_whitespace().collect();
                if parts.len() != 2 {
                    bot.send_message(msg.chat.id, "Usage: /set <user_id> <count>")
                        .await?;
                    return Ok(());
                }
                let target_id: i64 = match parts[0].parse() {
                    Ok(id) => id,
                    Err(_) => {
                        bot.send_message(msg.chat.id, "Invalid user ID.").await?;
                        return Ok(());
                    }
                };
                let count: i64 = match parts[1].parse() {
                    Ok(c) => c,
                    Err(_) => {
                        bot.send_message(msg.chat.id, "Invalid count.").await?;
                        return Ok(());
                    }
                };
                set_user_count(&pool, target_id, count).await?;
                bot.send_message(
                    msg.chat.id,
                    format!("User {} count set to {}.", target_id, count),
                )
                .await?;
            }
        }
        Command::Reset(arg) => {
            if let Some(user) = msg.from {
                if !is_admin(&pool, user.id.0 as i64).await.unwrap_or(false) {
                    bot.send_message(msg.chat.id, "Permission denied.").await?;
                    return Ok(());
                }
                let target_id: i64 = match arg.trim().parse() {
                    Ok(id) => id,
                    Err(_) => {
                        bot.send_message(msg.chat.id, "Usage: /reset <user_id>")
                            .await?;
                        return Ok(());
                    }
                };
                delete_user(&pool, target_id).await?;
                bot.send_message(msg.chat.id, format!("User {} removed.", target_id))
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
