/*
 * Copyright (C) 2026 VeryBaaad <verybaaad@outlook.com>
 * SPDX-License-Identifier: MIT
 */

use super::{Translation, ZwUserInfo};

pub struct ZhCn;

impl Translation for ZhCn {
    fn banned_message(&self) -> &'static str {
        "You have been permanently banned\n您已被永久封禁"
    }

    fn permission_denied(&self) -> &'static str {
        "Permission denied."
    }

    fn only_initiator_can_click(&self) -> &'static str {
        "只有发起人可以点击此按钮"
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
        format!("未找到用户 {} 的记录，无法进行帮助。", target_key)
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
            "{}，杂鱼杂鱼，已经达到顶峰了呢\\~\n\n\
             您在自慰排行榜上的位置：{}\n\
             总次数：{}次\n\
             下次可进行自慰的时间：{}分{}秒",
            user_mention, rank, count, cd_mins, cd_secs
        )
    }

    fn zw_self_success(&self, rank: usize, new_count: i64) -> String {
        format!(
            "已开始自慰！\n\n\
             您在自慰排行榜上的位置：{}\n\
             总次数：{}次\n\
             下次可进行自慰的时间：30分0秒",
            rank, new_count
        )
    }

    fn zw_cd_initiator(&self, mention: &str, cd_mins: i64, cd_secs: i64) -> String {
        format!("发起者 {} 仍在冷却：{}分{}秒", mention, cd_mins, cd_secs)
    }

    fn zw_cd_partner(&self, mention: &str, cd_mins: i64, cd_secs: i64) -> String {
        format!("另一位 {} 仍在冷却：{}分{}秒", mention, cd_mins, cd_secs)
    }

    fn zw_pair_cooldown(
        &self,
        initiator: &ZwUserInfo<'_>,
        target: &ZwUserInfo<'_>,
        cd_messages: &str,
    ) -> String {
        format!(
            "{}，杂鱼杂鱼，他好像昏厥了呢\n\n\
             发起者：{}\n\
             次数：{}次\n\
             排行榜位置：{}\n\n\
             另一位：{}\n\
             次数：{}次\n\
             排行榜位置：{}\n\n\
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
            "已进行双人运动！\n\n\
             {} 带上 {} 进行了性行为！\n\n\
             发起者：{}次\n\
             另一位：{}次\n\n\
             您在自慰排行榜上的位置：{}\n\
             另一位在自慰排行榜上的位置：{}\n\
             下次可进行自慰的时间：30分0秒",
            initiator.mention,
            target.mention,
            initiator.count,
            target.count,
            initiator.rank,
            target.rank
        )
    }

    fn rank_title(&self) -> &'static str {
        "自慰排行榜\n\n"
    }

    fn rank_prev_page(&self) -> &'static str {
        "上一页"
    }

    fn rank_next_page(&self) -> &'static str {
        "下一页"
    }

    fn rank_load_failed(&self) -> &'static str {
        "排行榜加载失败"
    }

    fn rank_line(&self, rank: usize, mention: &str, count: i64) -> String {
        format!("{}\\. {}: {}次\n", rank, mention, count)
    }

    fn inline_zw_text(&self) -> &'static str {
        "点击下方按钮进行紫薇\n直接爽4！"
    }

    fn inline_zw_title(&self) -> &'static str {
        "自慰"
    }

    fn inline_zw_description(&self) -> &'static str {
        "30分钟进行一次"
    }

    fn inline_rank_title(&self) -> &'static str {
        "排行榜"
    }

    fn inline_rank_description(&self) -> &'static str {
        "谁更多"
    }

    fn inline_version_title(&self) -> &'static str {
        "Bot 版本"
    }

    fn inline_version_description(&self) -> &'static str {
        "查看当前Bot版本"
    }

    fn inline_eunjeong_title(&self) -> &'static str {
        "恩！情！"
    }

    fn inline_eunjeong_description(&self) -> &'static str {
        "Eun! Jeong!"
    }

    fn inline_eunjeong_prefix(&self) -> &'static str {
        "恩！情！\n"
    }

    fn inline_zw_target_button(&self) -> &'static str {
        "自慰 (目标)"
    }

    fn inline_zw_target_title_fmt(&self, user_id: i64) -> String {
        format!("自慰 {}", user_id)
    }

    fn inline_zw_target_message_fmt(&self, user_id: i64) -> String {
        format!("对用户 {} 的操作", user_id)
    }

    fn error_retry_later(&self) -> &'static str {
        "发生错误，请稍后重试"
    }
}
