/*
 * Copyright (C) 2026 VeryBaaad <verybaaad@outlook.com>
 * SPDX-License-Identifier: MIT
 */
pub mod config;
pub mod db;
pub mod fun;
pub mod logger;

pub type DbPool = sqlx::Pool<sqlx::Any>;
pub type DbRow = sqlx::any::AnyRow;

pub use db::*;
