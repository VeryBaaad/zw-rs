/*
 * Copyright (C) 2026 VeryBaaad <verybaaad@outlook.com>
 * SPDX-License-Identifier: MIT
 */
use crate::services::{
    build_rank_keyboard, build_rank_text, calculate_page_info, handle_rank, process_zw_for_user,
    process_zw_help_for_user,
};
use crate::utils::DbPool;
use crate::utils::config::DatabaseKind;
use crate::utils::logger::log;
use log::Level;
use sqlx::Row;
use std::error::Error;
use teloxide::prelude::*;

pub async fn callback_handler(
    bot: Bot,
    q: CallbackQuery,
    pool: DbPool,
    database_kind: DatabaseKind,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(data) = &q.data {
        log(
            Level::Debug,
            "callback_handler",
            &format!("Received callback data: {}", data),
        );

        // rank pagination
        if let Some(stripped) = data.strip_prefix("rank_") {
            let page: usize = stripped.parse().unwrap_or(0);
            log(
                Level::Debug,
                "callback_handler",
                &format!("rank callback page: {}", page),
            );

            if let Some(msg) = &q.message {
                let chat_id = msg.chat().id;
                let message_id = msg.id();
                if let Err(e) = handle_rank(
                    bot.clone(),
                    chat_id,
                    Some(message_id),
                    None,
                    pool.clone(),
                    database_kind,
                    page,
                )
                .await
                {
                    log(
                        Level::Error,
                        "callback_handler",
                        &format!("handle_rank failed: {}", e),
                    );
                }
            } else if let Some(inline_id) = &q.inline_message_id {
                log(
                    Level::Debug,
                    "callback_handler",
                    &format!("rank callback editing inline_message_id {}", inline_id),
                );

                let total = sqlx::query("SELECT COUNT(*) as count FROM users")
                    .fetch_one(&pool)
                    .await?
                    .try_get::<i64, _>("count")? as usize;
                let (valid_page, offset) = calculate_page_info(total, page);

                let rank_query = match database_kind {
                    DatabaseKind::Sqlite | DatabaseKind::MySql | DatabaseKind::MariaDb => {
                        "SELECT user_id, username, first_name, last_name, count FROM users ORDER BY count DESC, last_time ASC LIMIT ? OFFSET ?"
                    }
                    DatabaseKind::Postgres => {
                        "SELECT user_id, username, first_name, last_name, count FROM users ORDER BY count DESC, last_time ASC LIMIT $1 OFFSET $2"
                    }
                };
                let rows = sqlx::query(rank_query)
                    .bind(10i64)
                    .bind(offset)
                    .fetch_all(&pool)
                    .await?;

                let text = build_rank_text(&rows, offset)?;
                let keyboard = build_rank_keyboard(valid_page, total);

                if let Err(e) = bot
                    .edit_message_text_inline(inline_id.as_str(), text)
                    .reply_markup(keyboard)
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                    .await
                {
                    log(
                        Level::Error,
                        "callback_handler",
                        &format!("edit_message_text_inline failed: {}", e),
                    );
                }
            } else {
                log(
                    Level::Warn,
                    "callback_handler",
                    "rank_ callback received but q.message and q.inline_message_id are None",
                );
            }
            let _ = bot.answer_callback_query(q.id).await;
            return Ok(());
        }

        if let Some(stripped) = data.strip_prefix("zw_self_") {
            log(Level::Debug, "callback_handler", "zw_self callback");
            let expected_initiator_id = match stripped.parse::<i64>() {
                Ok(id) => id,
                Err(_) => {
                    log(
                        Level::Warn,
                        "callback_handler",
                        &format!("Invalid zw_self callback data: {}", data),
                    );
                    let _ = bot.answer_callback_query(q.id).await;
                    return Ok(());
                }
            };

            let actual_initiator_id = q.from.id.0 as i64;
            if actual_initiator_id != expected_initiator_id {
                log(
                    Level::Warn,
                    "callback_handler",
                    &format!(
                        "Permission denied: {} tried to click zw_self initiated by {}",
                        actual_initiator_id, expected_initiator_id
                    ),
                );
                let _ = bot
                    .answer_callback_query(q.id)
                    .show_alert(true)
                    .text("只有发起人可以点击此按钮")
                    .await;
                return Ok(());
            }

            let from = &q.from;
            let user_id = from.id.0 as i64;
            let username = from.username.as_deref();
            let first_name = Some(from.first_name.as_str());
            let last_name = from.last_name.as_deref();

            // Sync user info from Telegram
            if let Err(e) = crate::utils::db::sync_user_info(
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
                    "callback_handler",
                    &format!("Failed to sync user info for {}: {}", user_id, e),
                );
            }

            match process_zw_for_user(
                &pool,
                database_kind,
                user_id,
                username,
                first_name,
                last_name,
            )
            .await
            {
                Ok((text, _)) => {
                    if let Some(msg) = &q.message {
                        log(
                            Level::Debug,
                            "callback_handler",
                            "zw_self: editing q.message",
                        );
                        let chat_id = msg.chat().id;
                        let message_id = msg.id();
                        if let Err(e) = bot
                            .edit_message_text(chat_id, message_id, text.clone())
                            .await
                        {
                            log(
                                Level::Error,
                                "callback_handler",
                                &format!("edit_message_text failed: {}", e),
                            );
                            if let Err(e2) = bot.send_message(chat_id, text.clone()).await {
                                log(
                                    Level::Error,
                                    "callback_handler",
                                    &format!("send_message fallback failed: {}", e2),
                                );
                            }
                        }
                    } else if let Some(inline_id) = &q.inline_message_id {
                        log(
                            Level::Debug,
                            "callback_handler",
                            &format!("zw_self: editing inline_message_id {}", inline_id),
                        );
                        if let Err(e) = bot.edit_message_text_inline(inline_id, text.clone()).await
                        {
                            log(
                                Level::Error,
                                "callback_handler",
                                &format!("edit_message_text_inline failed: {}", e),
                            );
                        }
                    } else {
                        log(
                            Level::Warn,
                            "callback_handler",
                            "zw_self: no q.message and no inline_message_id, sending DM",
                        );
                        if let Err(e) = bot.send_message(ChatId(user_id), text.clone()).await {
                            log(
                                Level::Error,
                                "callback_handler",
                                &format!("send DM failed: {}", e),
                            );
                        }
                    }
                }
                Err(e) => {
                    log(
                        Level::Error,
                        "callback_handler",
                        &format!("process_zw_for_user failed: {}", e),
                    );
                    if let Some(msg) = &q.message {
                        let _ = bot
                            .send_message(msg.chat().id, "发生错误，请稍后重试")
                            .await;
                    } else {
                        let _ = bot
                            .send_message(ChatId(user_id), "发生错误，请稍后重试")
                            .await;
                    }
                }
            }
            let _ = bot.answer_callback_query(q.id).await;
            return Ok(());
        }

        if let Some(stripped) = data.strip_prefix("zw_user_") {
            log(
                Level::Debug,
                "callback_handler",
                &format!("zw_user callback: {}", data),
            );
            // Parse target_id_initiator_id format
            let parts: Vec<&str> = stripped.split('_').collect();
            if parts.len() == 2 {
                if let (Ok(target_id), Ok(expected_initiator_id)) =
                    (parts[0].parse::<i64>(), parts[1].parse::<i64>())
                {
                    let from = &q.from;
                    let actual_initiator_id = from.id.0 as i64;

                    // Check permission: only allow the query initiator
                    if actual_initiator_id != expected_initiator_id {
                        log(
                            Level::Warn,
                            "callback_handler",
                            &format!(
                                "Permission denied: {} tried to use button initiated by {}",
                                actual_initiator_id, expected_initiator_id
                            ),
                        );
                        let _ = bot
                            .answer_callback_query(q.id)
                            .show_alert(true)
                            .text("只有发起人可以点击此按钮")
                            .await;
                        return Ok(());
                    }

                    let initiator_username = from.username.as_deref();
                    let initiator_first_name = Some(from.first_name.as_str());
                    let initiator_last_name = from.last_name.as_deref();

                    // Sync initiator user info
                    if let Err(e) = crate::utils::db::sync_user_info(
                        &pool,
                        database_kind,
                        actual_initiator_id,
                        initiator_username,
                        initiator_first_name,
                        initiator_last_name,
                    )
                    .await
                    {
                        log(
                            Level::Warn,
                            "callback_handler",
                            &format!(
                                "Failed to sync initiator info for {}: {}",
                                actual_initiator_id, e
                            ),
                        );
                    }

                    let (target_username, target_first_name, target_last_name) = match sqlx::query(
                        "SELECT username, first_name, last_name FROM users WHERE user_id = ?",
                    )
                    .bind(target_id)
                    .fetch_optional(&pool)
                    .await
                    {
                        Ok(Some(row)) => (
                            row.try_get::<String, _>("username").ok(),
                            row.try_get::<String, _>("first_name").ok(),
                            row.try_get::<String, _>("last_name").ok(),
                        ),
                        _ => (None, None, None),
                    };

                    match process_zw_help_for_user(
                        &pool,
                        database_kind,
                        actual_initiator_id,
                        initiator_username,
                        initiator_first_name,
                        initiator_last_name,
                        target_id,
                        target_username.as_deref(),
                        target_first_name.as_deref(),
                        target_last_name.as_deref(),
                    )
                    .await
                    {
                        Ok((text, success)) => {
                            if let Some(msg) = &q.message {
                                log(
                                    Level::Debug,
                                    "callback_handler",
                                    "zw_user: editing q.message",
                                );
                                let chat_id = msg.chat().id;
                                let message_id = msg.id();
                                if let Err(e) = bot
                                    .edit_message_text(chat_id, message_id, text.clone())
                                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                                    .await
                                {
                                    log(
                                        Level::Error,
                                        "callback_handler",
                                        &format!("edit_message_text failed: {}", e),
                                    );
                                    if let Err(e2) = bot
                                        .send_message(chat_id, text.clone())
                                        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                                        .await
                                    {
                                        log(
                                            Level::Error,
                                            "callback_handler",
                                            &format!("send_message fallback failed: {}", e2),
                                        );
                                    }
                                }
                            } else if let Some(inline_id) = &q.inline_message_id {
                                log(
                                    Level::Debug,
                                    "callback_handler",
                                    &format!("zw_user: editing inline_message_id {}", inline_id),
                                );
                                if let Err(e) = bot
                                    .edit_message_text_inline(inline_id, text.clone())
                                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                                    .await
                                {
                                    log(
                                        Level::Error,
                                        "callback_handler",
                                        &format!("edit_message_text_inline failed: {}", e),
                                    );
                                }
                            } else {
                                log(
                                    Level::Warn,
                                    "callback_handler",
                                    "zw_user callback but q.message and inline_message_id are None",
                                );
                            }
                            if success {
                                bot.send_message(UserId(target_id as u64), text)
                                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                                    .await?;
                            }
                        }
                        Err(e) => {
                            log(
                                Level::Error,
                                "callback_handler",
                                &format!("process_zw_help_for_user failed: {}", e),
                            );
                        }
                    }
                } else {
                    log(
                        Level::Warn,
                        "callback_handler",
                        &format!("Invalid zw_user format in callback: {}", data),
                    );
                }
            } else {
                log(
                    Level::Warn,
                    "callback_handler",
                    &format!("Invalid zw_user format in callback: {}", data),
                );
            }
            let _ = bot.answer_callback_query(q.id).await;
            return Ok(());
        }

        log(
            Level::Warn,
            "callback_handler",
            &format!("Unhandled callback data: {}", data),
        );
        let _ = bot.answer_callback_query(q.id).await;
    } else {
        log(
            Level::Debug,
            "callback_handler",
            "CallbackQuery has no data",
        );
    }
    Ok(())
}
