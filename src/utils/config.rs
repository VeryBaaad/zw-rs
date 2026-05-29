/*
 * Copyright (C) 2026 VeryBaaad <verybaaad@outlook.com>
 * SPDX-License-Identifier: MIT
 */

use crate::utils::logger::log;
use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_DATABASE_URL: &str = "sqlite:zw.db";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseKind {
    Sqlite,
    Postgres,
    MySql,
    MariaDb,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub teloxide_token: String,
    pub database_url: String,
    pub database_kind: DatabaseKind,
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
    let file_config = read_config_file()?;

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
    let database_url = normalize_database_url(database_url);
    let database_kind = database_kind_from_url(&database_url)?;

    Ok(RuntimeConfig {
        teloxide_token: token,
        database_url,
        database_kind,
    })
}

fn read_config_file() -> Result<Option<FileConfig>> {
    let mut candidates = vec![PathBuf::from("config.toml")];
    if let Some(exe_dir) = executable_dir() {
        candidates.push(exe_dir.join("config.toml"));
    }

    for p in candidates {
        if p.exists() {
            let content = fs::read_to_string(&p)
                .with_context(|| format!("Failed to read {}", p.display()))?;
            let parsed = toml::from_str::<FileConfig>(&content)
                .with_context(|| format!("Failed to parse {}", p.display()))?;
            return Ok(Some(parsed));
        }
    }

    log(
        log::Level::Debug,
        "read_config",
        "Config file not found, using defaults and environment variables",
    );
    Ok(None)
}

fn executable_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
}

fn normalize_database_url(url: String) -> String {
    const SQLITE_PREFIX: &str = "sqlite:";
    if !url.starts_with(SQLITE_PREFIX) {
        return url;
    }

    let raw_path = &url[SQLITE_PREFIX.len()..];
    if raw_path.is_empty() || raw_path == ":memory:" {
        return url;
    }

    let db_path = Path::new(raw_path);
    if db_path.is_absolute() {
        return url;
    }

    if let Ok(current_dir) = std::env::current_dir() {
        let absolute = current_dir.join(db_path);
        return format!("{SQLITE_PREFIX}{}", absolute.display());
    }

    if let Some(exe_dir) = executable_dir() {
        let absolute = exe_dir.join(db_path);
        return format!("{SQLITE_PREFIX}{}", absolute.display());
    }

    url
}

fn database_kind_from_url(url: &str) -> Result<DatabaseKind> {
    if url.starts_with("sqlite:") {
        return Ok(DatabaseKind::Sqlite);
    }
    if url.starts_with("postgres:") || url.starts_with("postgresql:") {
        return Ok(DatabaseKind::Postgres);
    }
    if url.starts_with("mysql:") {
        return Ok(DatabaseKind::MySql);
    }
    if url.starts_with("mariadb:") {
        return Ok(DatabaseKind::MariaDb);
    }

    Err(anyhow!(
        "Unsupported database URL scheme. Expected sqlite, postgres, mysql, or mariadb"
    ))
}
