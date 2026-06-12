/*
 * Copyright (C) 2026 VeryBaaad <verybaaad@outlook.com>
 * SPDX-License-Identifier: MIT
 */

pub mod en;
pub mod zh_cn;

/// Supported locales.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    ZhCn,
    En,
}

impl Locale {
    /// Detect locale from Telegram's `language_code` (ISO 639-1 or IETF BCP 47).
    pub fn from_language_code(code: Option<&str>) -> Self {
        match code {
            Some(c) if c.starts_with("zh") => Locale::ZhCn,
            Some(c) if c.starts_with("en") => Locale::En,
            // Fallback to Chinese for any other unsupported language or missing code.
            _ => Locale::ZhCn,
        }
    }
}

/// Aggregated user info for pair-related i18n messages.
pub struct ZwUserInfo<'a> {
    pub mention: &'a str,
    pub count: i64,
    pub rank: usize,
}

/// All user-facing strings. Each language module implements this trait.
/// Methods that need runtime parameters return `String`; static texts may
/// return `&'static str` to avoid unnecessary allocation when the caller
/// does not need ownership.
pub trait Translation: Send + Sync {
    // ── Ban ────────────────────────────────────────────────────────
    fn banned_message(&self) -> &'static str;

    // ── Permission ─────────────────────────────────────────────────
    fn permission_denied(&self) -> &'static str;
    fn only_initiator_can_click(&self) -> &'static str;

    // ── Admin /set ─────────────────────────────────────────────────
    fn set_usage(&self) -> &'static str;
    fn invalid_user_id(&self) -> &'static str;
    fn invalid_count(&self) -> &'static str;
    fn user_count_set(&self, target_id: i64, count: i64) -> String;

    // ── Admin /reset ───────────────────────────────────────────────
    fn reset_usage(&self) -> &'static str;
    fn user_removed(&self, target_id: i64) -> String;

    // ── Admin /continue ────────────────────────────────────────────
    fn continue_usage(&self) -> &'static str;
    fn user_last_time_set(&self, target_id: i64) -> String;

    // ── ZW (self) ──────────────────────────────────────────────────
    fn user_not_found(&self, target_key: &str) -> String;
    fn zw_self_cooldown(
        &self,
        user_mention: &str,
        rank: usize,
        count: i64,
        cd_mins: i64,
        cd_secs: i64,
    ) -> String;
    fn zw_self_success(&self, rank: usize, new_count: i64) -> String;

    // ── ZW (pair) ──────────────────────────────────────────────────
    fn zw_cd_initiator(&self, mention: &str, cd_mins: i64, cd_secs: i64) -> String;
    fn zw_cd_partner(&self, mention: &str, cd_mins: i64, cd_secs: i64) -> String;
    fn zw_pair_cooldown(
        &self,
        initiator: &ZwUserInfo<'_>,
        target: &ZwUserInfo<'_>,
        cd_messages: &str,
    ) -> String;
    fn zw_pair_success(&self, initiator: &ZwUserInfo<'_>, target: &ZwUserInfo<'_>) -> String;

    // ── Rank ───────────────────────────────────────────────────────
    fn rank_title(&self) -> &'static str;
    fn rank_prev_page(&self) -> &'static str;
    fn rank_next_page(&self) -> &'static str;
    fn rank_load_failed(&self) -> &'static str;
    /// Format for a single rank line: `rank`, `mention`, `count`.
    fn rank_line(&self, rank: usize, mention: &str, count: i64) -> String;

    // ── Inline query ───────────────────────────────────────────────
    fn inline_zw_text(&self) -> &'static str;
    fn inline_zw_title(&self) -> &'static str;
    fn inline_zw_description(&self) -> &'static str;
    fn inline_rank_title(&self) -> &'static str;
    fn inline_rank_description(&self) -> &'static str;
    fn inline_version_title(&self) -> &'static str;
    fn inline_version_description(&self) -> &'static str;
    fn inline_eunjeong_title(&self) -> &'static str;
    fn inline_eunjeong_description(&self) -> &'static str;
    fn inline_eunjeong_prefix(&self) -> &'static str;
    fn inline_zw_target_button(&self) -> &'static str;
    fn inline_zw_target_title_fmt(&self, user_id: i64) -> String;
    fn inline_zw_target_message_fmt(&self, user_id: i64) -> String;

    // ── Error ──────────────────────────────────────────────────────
    fn error_retry_later(&self) -> &'static str;
}

/// Return the translation instance for the given locale.
pub fn get_translation(locale: Locale) -> &'static dyn Translation {
    match locale {
        Locale::ZhCn => &zh_cn::ZhCn,
        Locale::En => &en::En,
    }
}
