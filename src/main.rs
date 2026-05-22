/*
 * Copyright (C) 2026 VeryBaaad <verybaaad@outlook.com>
 * SPDX-License-Identifier: MIT
 */

mod handlers;
mod services;
mod utils;
#[cfg(target_os = "windows")]
mod windows_service;

use anyhow::Context;
use handlers::commands::Command;
use handlers::{callback_handler, commands_handler, inline_query_handler};
use log::Level;
use sqlx::SqlitePool;
use teloxide::prelude::*;
use tokio::sync::watch;
use utils::config::load_runtime_config;
use utils::db::init_database;
use utils::logger::log;

#[tokio::main]
async fn main() {
    #[cfg(target_os = "windows")]
    {
        match windows_service::try_run_as_service() {
            Ok(true) => return,
            Ok(false) => {}
            Err(err) => {
                log(
                    Level::Error,
                    "ZWBotDaemon",
                    &format!("Failed while starting Windows service path: {err:#}"),
                );
                std::process::exit(1);
            }
        }
    }
    if let Err(err) = run_bot(true, None).await {
        log(
            Level::Error,
            "ZWBotDaemon",
            &format!("Fatal startup error: {err:#}"),
        );
        std::process::exit(1);
    }
}

pub async fn run_bot(
    enable_ctrlc_handler: bool,
    mut shutdown_rx: Option<watch::Receiver<bool>>,
) -> anyhow::Result<()> {
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

    let config = load_runtime_config()?;
    let bot = Bot::new(config.teloxide_token);

    let database_url = config.database_url;
    log(
        Level::Info,
        "ZWBotDaemon",
        &format!("Connecting to database: {}", database_url),
    );
    let pool = SqlitePool::connect(&database_url)
        .await
        .with_context(|| format!("Failed to connect to database: {database_url}"))?;
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

    let mut dispatcher = Dispatcher::builder(bot, handler).dependencies(dptree::deps![pool]);

    if enable_ctrlc_handler {
        dispatcher = dispatcher.enable_ctrlc_handler();
    }

    let mut dispatcher = dispatcher.build();
    let shutdown_token = dispatcher.shutdown_token();
    let dispatch_task = tokio::spawn(async move {
        dispatcher.dispatch().await;
    });

    if let Some(rx) = shutdown_rx.as_mut()
        && rx.changed().await.is_ok()
        && *rx.borrow()
    {
        let _ = shutdown_token.shutdown();
    }
    let _ = dispatch_task.await;

    Ok(())
}
