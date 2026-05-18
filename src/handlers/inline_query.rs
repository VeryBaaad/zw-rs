/*
 * Copyright (C) 2026 VeryBaaad <verybaaad@outlook.com>
 * SPDX-License-Identifier: MIT
 */
use crate::handlers::commands::get_version_info;
use crate::services::{build_rank_keyboard, build_rank_text, calculate_page_info};
use crate::utils::db::ban_status;
use crate::utils::get_total_users;
use crate::utils::logger::log;
use crate::utils::user_exists;
use log::Level;
use sqlx::SqlitePool;
use std::error::Error;
use teloxide::{
    prelude::*,
    types::{InlineQuery, InlineQueryResult, InlineQueryResultArticle, InputMessageContent},
};
use tokio::time::{sleep, Duration};
use rand::rng;
use rand::RngExt;

pub async fn inline_query_handler(
    bot: Bot,
    q: InlineQuery,
    pool: SqlitePool,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    log(
        Level::Debug,
        "inline_query_handler",
        &format!("Received inline query: '{}'", q.query),
    );
    let query = q.query.trim();
    let mut results: Vec<InlineQueryResult> = Vec::new();

    // when banned
    if ban_status(&pool, q.from.id.0 as i64).await? == 1 {
        let ban_article = InlineQueryResultArticle::new(
            "banned",
            "You have been permanently banned",
            InputMessageContent::Text(teloxide::types::InputMessageContentText {
                message_text: "You have been permanently banned\n您已被永久封禁".to_string(),
                parse_mode: None,
                entities: None,
                link_preview_options: None,
            }),
        )
        .description("您已被永久封禁");
        results.push(InlineQueryResult::Article(ban_article));
    }
    // Extract page from query if it's a number and not a user_id
    let rank_page = if !query.is_empty() {
        query
            .parse::<usize>()
            .ok()
            .map(|p| if p > 0 { p - 1 } else { 0 })
            .unwrap_or(0)
    } else {
        0
    };

    // Generate rank text and keyboard
    let rank_text = async {
        match async {
            let total = match get_total_users(&pool).await {
                Ok(t) => t as usize,
                Err(e) => return Err::<String, Box<dyn Error + Send + Sync>>(e),
            };
            let (_valid_page, offset) = calculate_page_info(total, rank_page);
            let rows = sqlx::query(
                "SELECT user_id, username, count FROM users ORDER BY count DESC, last_time ASC LIMIT ? OFFSET ?"
            )
            .bind(10i64)
            .bind(offset)
            .fetch_all(&pool)
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

            build_rank_text(&rows, offset)
                .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
        }.await {
            Ok(t) => t,
            Err(e) => {
                log(Level::Error, "inline_query_handler", &format!("Generate rank text error: {}", e));
                "排行榜加载失败".to_string()
            }
        }
    }.await;

    let initiator_id = q.from.id.0 as i64;
    let zw_text = "点击下方按钮进行紫薇\n直接爽4！";
    let mut zw_kb = teloxide::types::InlineKeyboardMarkup::default();
    zw_kb
        .inline_keyboard
        .push(vec![teloxide::types::InlineKeyboardButton::callback(
            "自慰",
            format!("zw_self_{}", initiator_id),
        )]);
    let zw_article = InlineQueryResultArticle::new(
        format!("zw_{}", chrono::Utc::now().timestamp_millis()),
        "自慰",
        InputMessageContent::Text(teloxide::types::InputMessageContentText {
            message_text: zw_text.to_string(),
            parse_mode: None,
            entities: None,
            link_preview_options: None,
        }),
    )
    .description("30分钟进行一次")
    .reply_markup(zw_kb);
    results.push(InlineQueryResult::Article(zw_article));

    // Extract page from query if it's a number and not a user_id
    let rank_page = if !query.is_empty() {
        query
            .parse::<usize>()
            .ok()
            .map(|p| if p > 0 { p - 1 } else { 0 })
            .unwrap_or(0)
    } else {
        0
    };

    let rank_keyboard = {
        let total = match get_total_users(&pool).await {
            Ok(t) => t as usize,
            Err(e) => {
                log(
                    Level::Error,
                    "inline_query_handler",
                    &format!("get_total_users error: {}", e),
                );
                0
            }
        };
        build_rank_keyboard(rank_page, total)
    };
    let rank_article = InlineQueryResultArticle::new(
        format!("rank_{}", chrono::Utc::now().timestamp_millis()),
        "排行榜",
        InputMessageContent::Text(teloxide::types::InputMessageContentText {
            message_text: rank_text,
            parse_mode: None,
            entities: None,
            link_preview_options: None,
        }),
    )
    .description("谁更多")
    .reply_markup(rank_keyboard);
    log(
        Level::Debug,
        "inline_query_handler",
        &format!(
            "Answering inline query: results={}, rank_kb_rows={}",
            results.len(),
            0
        ),
    );
    results.push(InlineQueryResult::Article(rank_article));

    let version_info = get_version_info().await?;
    let version_article = InlineQueryResultArticle::new(
        format!("version_{}", chrono::Utc::now().timestamp_millis()),
        "Bot 版本",
        InputMessageContent::Text(teloxide::types::InputMessageContentText {
            message_text: version_info,
            parse_mode: None,
            entities: None,
            link_preview_options: None,
        }),
    )
    .description("查看当前Bot版本");
    results.push(InlineQueryResult::Article(version_article));

    if !query.is_empty()
        && let Ok(user_id) = query.parse::<i64>()
        && user_exists(&pool, user_id).await?
    {
        let initiator_id = q.from.id.0 as i64;
        let mut kb = teloxide::types::InlineKeyboardMarkup::default();
        kb.inline_keyboard
            .push(vec![teloxide::types::InlineKeyboardButton::callback(
                "自慰 (目标)",
                format!("zw_user_{}_{}", user_id, initiator_id),
            )]);
        let art = InlineQueryResultArticle::new(
            format!("zw_user_{}_{}", user_id, initiator_id),
            format!("自慰 {}", user_id),
            InputMessageContent::Text(teloxide::types::InputMessageContentText {
                message_text: format!("对用户 {} 的操作", user_id),
                parse_mode: None,
                entities: None,
                link_preview_options: None,
            }),
        )
        .reply_markup(kb);
        results.push(InlineQueryResult::Article(art));
    }

    if ban_status(&pool, q.from.id.0 as i64).await? == 2 {
        let millis: u64 = rng().random_range(3000..=10000);
        sleep(Duration::from_millis(millis)).await;
    }

    if let Err(e) = bot
        .answer_inline_query(q.id, results)
        .is_personal(true)
        .cache_time(0)
        .await
    {
        log(
            Level::Error,
            "inline_query_handler",
            &format!("Failed to answer inline query: {}", e),
        );
    }
    Ok(())
}
