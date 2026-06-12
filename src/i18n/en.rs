/*
 * Copyright (C) 2026 VeryBaaad <verybaaad@outlook.com>
 * SPDX-License-Identifier: MIT
 */

use super::{Translation, ZwUserInfo};

pub struct En;

impl Translation for En {
    fn banned_message(&self) -> &'static str {
        "You have been permanently banned."
    }

    fn permission_denied(&self) -> &'static str {
        "Permission denied."
    }

    fn only_initiator_can_click(&self) -> &'static str {
        "Only the initiator can click this button."
    }

    fn set_usage(&self) -> &'static str {
        "Usage: /set <user_id> <count>"
    }

    fn invalid_user_id(&self) -> &'static str {
        "Invalid user ID."
    }

    fn invalid_count(&self) -> &'static str {
        "Invalid count."
    }

    fn user_count_set(&self, target_id: i64, count: i64) -> String {
        format!("User {} count set to {}.", target_id, count)
    }

    fn reset_usage(&self) -> &'static str {
        "Usage: /reset <user_id>"
    }

    fn user_removed(&self, target_id: i64) -> String {
        format!("User {} removed.", target_id)
    }

    fn continue_usage(&self) -> &'static str {
        "Usage: /continue <user_id>"
    }

    fn user_last_time_set(&self, target_id: i64) -> String {
        format!("User {} last_time set to 0.", target_id)
    }

    fn user_not_found(&self, target_key: &str) -> String {
        format!("User {} not found, cannot help.", target_key)
    }

    fn zw_self_cooldown(
        &self,
        user_mention: &str,
        rank: usize,
        count: i64,
        cd_mins: i64,
        cd_secs: i64,
    ) -> String {
        format!(
            "{}, you've already reached the peak\\~\n\n\
             Your rank: {}\n\
             Total: {} times\n\
             Next available in: {}m{}s",
            user_mention, rank, count, cd_mins, cd_secs
        )
    }

    fn zw_self_success(&self, rank: usize, new_count: i64) -> String {
        format!(
            "Started!\n\n\
             Your rank: {}\n\
             Total: {} times\n\
             Next available in: 30m0s",
            rank, new_count
        )
    }

    fn zw_cd_initiator(&self, mention: &str, cd_mins: i64, cd_secs: i64) -> String {
        format!(
            "Initiator {} is still cooling down: {}m{}s",
            mention, cd_mins, cd_secs
        )
    }

    fn zw_cd_partner(&self, mention: &str, cd_mins: i64, cd_secs: i64) -> String {
        format!(
            "Partner {} is still cooling down: {}m{}s",
            mention, cd_mins, cd_secs
        )
    }

    fn zw_pair_cooldown(
        &self,
        initiator: &ZwUserInfo<'_>,
        target: &ZwUserInfo<'_>,
        cd_messages: &str,
    ) -> String {
        format!(
            "{}, looks like they passed out\\~\n\n\
             Initiator: {}\n\
             Count: {} times\n\
             Rank: {}\n\n\
             Partner: {}\n\
             Count: {} times\n\
             Rank: {}\n\n\
             {}",
            initiator.mention,
            initiator.mention,
            initiator.count,
            initiator.rank,
            target.mention,
            target.count,
            target.rank,
            cd_messages
        )
    }

    fn zw_pair_success(&self, initiator: &ZwUserInfo<'_>, target: &ZwUserInfo<'_>) -> String {
        format!(
            "Duo action complete!\n\n\
             {} brought {} into the act!\n\n\
             Initiator: {} times\n\
             Partner: {} times\n\n\
             Your rank: {}\n\
             Partner's rank: {}\n\
             Next available in: 30m0s",
            initiator.mention,
            target.mention,
            initiator.count,
            target.count,
            initiator.rank,
            target.rank
        )
    }

    fn rank_title(&self) -> &'static str {
        "Leaderboard\n\n"
    }

    fn rank_prev_page(&self) -> &'static str {
        "Prev"
    }

    fn rank_next_page(&self) -> &'static str {
        "Next"
    }

    fn rank_load_failed(&self) -> &'static str {
        "Failed to load leaderboard."
    }

    fn rank_line(&self, rank: usize, mention: &str, count: i64) -> String {
        format!("{}\\. {}: {} times\n", rank, mention, count)
    }

    fn inline_zw_text(&self) -> &'static str {
        "Click the button below!\nGo ahead!"
    }

    fn inline_zw_title(&self) -> &'static str {
        "Go"
    }

    fn inline_zw_description(&self) -> &'static str {
        "Once every 30 minutes"
    }

    fn inline_rank_title(&self) -> &'static str {
        "Leaderboard"
    }

    fn inline_rank_description(&self) -> &'static str {
        "Who has more?"
    }

    fn inline_version_title(&self) -> &'static str {
        "Bot Version"
    }

    fn inline_version_description(&self) -> &'static str {
        "View current bot version"
    }

    fn inline_eunjeong_title(&self) -> &'static str {
        "Eun! Jeong!"
    }

    fn inline_eunjeong_description(&self) -> &'static str {
        "Eun! Jeong!"
    }

    fn inline_eunjeong_prefix(&self) -> &'static str {
        "Eun! Jeong!\n"
    }

    fn inline_zw_target_button(&self) -> &'static str {
        "Go (target)"
    }

    fn inline_zw_target_title_fmt(&self, user_id: i64) -> String {
        format!("Go {}", user_id)
    }

    fn inline_zw_target_message_fmt(&self, user_id: i64) -> String {
        format!("Action on user {}", user_id)
    }

    fn error_retry_later(&self) -> &'static str {
        "An error occurred, please try again later."
    }
}
