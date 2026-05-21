/*
 * Copyright (C) 2026 VeryBaaad <verybaaad@outlook.com>
 * SPDX-License-Identifier: MIT
 */

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::Path;

const DEFAULT_DATABASE_URL: &str = "sqlite:zw.db";

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub teloxide_token: String,
    pub database_url: String,
}

#[derive(Debug, Deserialize)]
struct FileConfig {
    bot: Option<BotConfig>,
    database: Option<DatabaseConfig>,
}

#[derive(Debug, Deserialize)]
struct BotConfig {
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DatabaseConfig {
    url: Option<String>,
}

pub fn load_runtime_config() -> Result<RuntimeConfig> {
    let file_config = read_config_file("config.toml")?;

    let token = file_config
        .as_ref()
        .and_then(|c| c.bot.as_ref())
        .and_then(|b| b.token.as_ref())
        .cloned()
        .or_else(|| env::var("TELOXIDE_TOKEN").ok())
        .ok_or_else(|| anyhow!("Missing bot token. Configure bot.token in config.toml or TELOXIDE_TOKEN in environment"))?;

    let database_url = file_config
        .as_ref()
        .and_then(|c| c.database.as_ref())
        .and_then(|d| d.url.as_ref())
        .cloned()
        .or_else(|| env::var("DATABASE_URL").ok())
        .unwrap_or_else(|| DEFAULT_DATABASE_URL.to_string());

    Ok(RuntimeConfig {
        teloxide_token: token,
        database_url,
    })
}

fn read_config_file(path: &str) -> Result<Option<FileConfig>> {
    let p = Path::new(path);
    if !p.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(p).with_context(|| format!("Failed to read {}", path))?;
    let parsed = toml::from_str::<FileConfig>(&content)
        .with_context(|| format!("Failed to parse {}", path))?;
    Ok(Some(parsed))
}
