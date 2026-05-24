/*
 * Copyright (C) 2026 VeryBaaad <verybaaad@outlook.com>
 * SPDX-License-Identifier: MIT
 */
use rand::RngExt;
use rand::rng;

pub async fn eunjeong_generate(custom_count: Option<usize>) -> String {
    let count = match custom_count {
        Some(n) => n.max(1),
        None => rng().random_range(1..=25),
    };
    r"\o/".repeat(count)
}
