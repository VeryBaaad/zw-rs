/*
 * Copyright (C) 2026 VeryBaaad <verybaaad@outlook.com>
 * SPDX-License-Identifier: MIT
 */
use crate::services::{handle_zw, handle_rank};
use crate::utils::logger::log;
use log::Level;
use sqlx::SqlitePool;
use std::error::Error;
use teloxide::{prelude::*, utils::command::BotCommands};

#[derive(BotCommands, Clone, Debug)]
#[command(rename_rule = "lowercase")]
pub enum Command {
    Zw(String),
    Rank(String),
    Version,
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
