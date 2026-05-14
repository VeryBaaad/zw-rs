/*
 * Copyright (C) 2026 VeryBaaad <verybaaad@outlook.com>
 * SPDX-License-Identifier: MIT
 */
use crate::utils::logger::log;
use log::Level;
use sqlx::{Row, SqlitePool};
use std::error::Error;
use teloxide::{
    prelude::*,
    types::{InlineKeyboardMarkup, MessageId, ReplyParameters},
};

const PER_PAGE: i64 = 10;
const RANK_TITLE: &str = "自慰排行榜\n\n";

/// Calculate pagination info from total count and page number
pub fn calculate_page_info(total: usize, page: usize) -> (usize, i64) {
    let max_page_index = if total > 0 {
        ((total as f64 / PER_PAGE as f64).ceil() as usize) - 1
    } else {
        0
    };
    let valid_page = if page <= max_page_index { page } else { 0 };
    let offset: i64 = (valid_page as i64) * PER_PAGE;
    (valid_page, offset)
}

/// Build rank text from database rows
pub fn build_rank_text(rows: &[sqlx::sqlite::SqliteRow], offset: i64) -> Result<String, sqlx::Error> {
    let mut text = RANK_TITLE.to_string();
    for (i, row) in rows.iter().enumerate() {
        let rank = (offset + i as i64 + 1) as usize;
        let username: String = row.try_get("username")?;
        let count: i64 = row.try_get("count")?;
        let user_id: i64 = row.try_get("user_id")?;
        text.push_str(&format!(
            "{}. {}: {}次\n{}\n",
            rank, username, count, user_id
        ));
    }
    Ok(text)
}

/// Build pagination keyboard
pub fn build_rank_keyboard(valid_page: usize, total: usize) -> InlineKeyboardMarkup {
    let mut keyboard = InlineKeyboardMarkup::default();
    let mut row = Vec::new();
    if valid_page > 0 {
        row.push(teloxide::types::InlineKeyboardButton::callback(
            "上一页",
            format!("rank_{}", valid_page - 1),
        ));
    }
    if (valid_page + 1) * (PER_PAGE as usize) < total {
        row.push(teloxide::types::InlineKeyboardButton::callback(
            "下一页",
            format!("rank_{}", valid_page + 1),
        ));
    }
    if !row.is_empty() {
        keyboard.inline_keyboard.push(row);
    }
    keyboard
}

pub async fn handle_rank(
    bot: Bot,
    chat_id: ChatId,
    message_id: Option<MessageId>,
    reply_to: Option<MessageId>,
    pool: SqlitePool,
    page: usize,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    log(
        Level::Info,
        "handle_rank",
        &format!("Handling rank command: page={}", page),
    );

    log(Level::Debug, "handle_rank", "Querying total user count");
    let total = sqlx::query("SELECT COUNT(*) as count FROM users")
        .fetch_one(&pool)
        .await?
        .try_get::<i64, _>("count")? as usize;
    log(
        Level::Debug,
        "handle_rank",
        &format!("Total users in database: {}", total),
    );

    let (valid_page, offset) = calculate_page_info(total, page);
    log(
        Level::Debug,
        "handle_rank",
        &format!(
            "Fetching rankings: per_page={}, offset={}",
            PER_PAGE, offset
        ),
    );

    log(Level::Debug, "handle_rank", "Querying users from database");
    let rows = sqlx::query(
        "SELECT user_id, username, count FROM users ORDER BY count DESC, last_time ASC LIMIT ? OFFSET ?"
    )
    .bind(PER_PAGE)
    .bind(offset)
    .fetch_all(&pool)
    .await?;
    log(
        Level::Debug,
        "handle_rank",
        &format!("Retrieved {} users from database", rows.len()),
    );

    let text = build_rank_text(&rows, offset)?;
    let keyboard = build_rank_keyboard(valid_page, total);

    if let Some(message_id) = message_id {
        log(Level::Debug, "handle_rank", "Editing existing rank message");
        if let Err(e) = bot
            .edit_message_text(chat_id, message_id, text)
            .reply_markup(keyboard)
            .await
        {
            log(
                Level::Error,
                "handle_rank",
                &format!("Failed to edit rank message: {}", e),
            );
            return Err(Box::new(e));
        }
    } else {
        log(Level::Debug, "handle_rank", "Sending new rank message");
        let mut req = bot.send_message(chat_id, text).reply_markup(keyboard);
        if let Some(reply_id) = reply_to {
            req = req.reply_parameters(ReplyParameters::new(reply_id));
        }
        if let Err(e) = req.await {
            log(
                Level::Error,
                "handle_rank",
                &format!("Failed to send rank message: {}", e),
            );
            return Err(Box::new(e));
        }
    }
    log(
        Level::Debug,
        "handle_rank",
        "Rank message sent successfully",
    );
    Ok(())
}

