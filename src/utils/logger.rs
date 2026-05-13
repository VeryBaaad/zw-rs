/*
 * Copyright (C) 2026 VeryBaaad <verybaaad@outlook.com>
 * SPDX-License-Identifier: MIT
 */
use log::Level;

pub fn log(priority: Level, tag: &str, msg: &str) {
    if priority == Level::Debug && !cfg!(debug_assertions) {
        return;
    }
    let shortlevel: &str = match priority {
        Level::Error => "E",
        Level::Warn => "W",
        Level::Info => "I",
        Level::Debug => "D",
        _ => "N",
    };
    let output: String = format!(
        "[ {} {}/{} ] {}",
        chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f"),
        shortlevel,
        tag,
        msg
    );
    log::log!(priority, "{}", output);
    println!("{}", output);
}
