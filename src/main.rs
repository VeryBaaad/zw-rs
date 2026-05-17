/*
 * Copyright (C) 2026 VeryBaaad <verybaaad@outlook.com>
 * SPDX-License-Identifier: MIT
 */

mod handlers;
mod services;
mod utils;

use handlers::commands::Command;
use handlers::{callback_handler, commands_handler, inline_query_handler};
use log::Level;
use sqlx::SqlitePool;
use std::env;
use teloxide::prelude::*;
use utils::db::init_database;
use utils::logger::log;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    log(
        Level::Info,
        "ZWBotDaemon",
        format!(
            "Starting zw-rs v{} ({}) (commit {}, built at {}) for {}",
            env!("CARGO_PKG_VERSION"),
            env!("VER_CODE"),
            env!("GIT_HASH"),
            env!("BUILD_TIME"),
            env!("BUILD_TARGET")
        )
        .as_str(),
    );

    let bot = Bot::from_env();

    let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:zw.db".to_string());
    log(
        Level::Info,
        "ZWBotDaemon",
        &format!("Connecting to database: {}", database_url),
    );
    let pool = SqlitePool::connect(&database_url)
        .await
        .expect("Failed to connect to database");
    log(
        Level::Info,
        "ZWBotDaemon",
        "Database connected successfully",
    );

    // init the database (create tables if not exist)
    init_database(&pool).await;

    let handler = dptree::entry()
        .branch(
            Update::filter_message()
                .filter_command::<Command>()
                .endpoint(commands_handler),
        )
        .branch(Update::filter_callback_query().endpoint(callback_handler))
        .branch(Update::filter_inline_query().endpoint(inline_query_handler));

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![pool])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}
