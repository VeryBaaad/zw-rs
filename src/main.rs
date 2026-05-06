use std::env;
use teloxide::{prelude::*, types::{InlineKeyboardMarkup, MessageId, ReplyParameters}, utils::command::BotCommands};
use sqlx::{SqlitePool, Row};
use chrono::{Utc, Duration};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum Command {
    Zw,
    Rank,
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    log::info!("Starting bot...");

    let bot = Bot::from_env();

    let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:zw.db".to_string());
    let pool = SqlitePool::connect(&database_url).await.expect("Failed to connect to database");

    // Create table if not exists
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY,
            user_id INTEGER UNIQUE,
            username TEXT,
            count INTEGER DEFAULT 0,
            last_time DATETIME
        )"
    )
    .execute(&pool)
    .await
    .expect("Failed to create table");

    let handler = dptree::entry()
        .branch(Update::filter_message().filter_command::<Command>().endpoint(commands_handler))
        .branch(Update::filter_callback_query().endpoint(callback_handler));

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![pool])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

async fn commands_handler(
    bot: Bot,
    msg: Message,
    cmd: Command,
    pool: SqlitePool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match cmd {
        Command::Zw => handle_zw(bot, msg, pool).await?,
        Command::Rank => handle_rank(bot, msg.chat.id, None, Some(msg.id), pool, 0).await?,
    }
    Ok(())
}

async fn handle_zw(
    bot: Bot,
    msg: Message,
    pool: SqlitePool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let user = msg.from.as_ref().unwrap();
    let user_id = user.id.0 as i64;
    let username = user.username.as_deref().unwrap_or("未知用户");
    let name = match user.last_name.as_deref() {
        Some(last_name) => format!("{} {}", user.first_name, last_name),
        None => user.first_name.clone(),
    };

    let now = Utc::now();
    let cd_duration = Duration::minutes(30);

    // Check if user exists
    let row = sqlx::query("SELECT count, last_time FROM users WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(&pool)
        .await?;

    let (current_count, last_time) = if let Some(row) = row {
        let count: i64 = row.try_get("count")?;
        let last_time: chrono::DateTime<Utc> = row.try_get("last_time")?;
        (count, Some(last_time))
    } else {
        (0, None)
    };

    if let Some(last_time) = last_time {
        let next_time = last_time + cd_duration;
        if now < next_time {
            let remaining = next_time - now;
            let mins = remaining.num_minutes();
            let secs = remaining.num_seconds() % 60;
            let rank = get_rank(&pool, user_id).await?;
            let text = format!(
                "{}，杂鱼杂鱼，已经达到顶峰了呢~\n\n您在自慰排行榜上的位置：{}\n总次数：{}次\n下次可进行自慰的时间：{}分{}秒",
                name, rank, current_count, mins, secs
            );
            bot.send_message(msg.chat.id, text)
                .reply_parameters(ReplyParameters::new(msg.id))
                .await?;
            return Ok(());
        }
    }

    // Update count and last_time
    let new_count = current_count + 1;
    sqlx::query(
        "INSERT INTO users (user_id, username, count, last_time) VALUES (?, ?, ?, ?)
         ON CONFLICT(user_id) DO UPDATE SET
         username = excluded.username,
         count = excluded.count,
         last_time = excluded.last_time"
    )
    .bind(user_id)
    .bind(username)
    .bind(new_count)
    .bind(now)
    .execute(&pool)
    .await?;

    let rank = get_rank(&pool, user_id).await?;
    let text = format!(
        "已开始自慰！\n\n您在自慰排行榜上的位置：{}\n总次数：{}次\n下次可进行自慰的时间：30分0秒",
        rank, new_count
    );
    bot.send_message(msg.chat.id, text)
        .reply_parameters(ReplyParameters::new(msg.id))
        .await?;
    Ok(())
}

async fn handle_rank(
    bot: Bot,
    chat_id: ChatId,
    message_id: Option<MessageId>,
    reply_to: Option<MessageId>,
    pool: SqlitePool,
    page: usize,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let per_page: i64 = 10;
    let offset: i64 = (page as i64) * per_page;

    let rows = sqlx::query(
        "SELECT user_id, username, count FROM users ORDER BY count DESC, last_time ASC LIMIT ? OFFSET ?"
    )
    .bind(per_page)
    .bind(offset)
    .fetch_all(&pool)
    .await?;

    let total = sqlx::query("SELECT COUNT(*) as count FROM users")
        .fetch_one(&pool)
        .await?
        .try_get::<i64, _>("count")? as usize;

    let mut text = "自慰排行榜\n\n".to_string();
    for (i, row) in rows.iter().enumerate() {
        let rank = (offset + i as i64 + 1) as usize;
        let username: String = row.try_get("username")?;
        let count: i64 = row.try_get("count")?;
        let user_id: i64 = row.try_get("user_id")?;
        text.push_str(&format!("{}. {}: {}次\n{}\n", rank, username, count, user_id));
    }

    let mut keyboard = InlineKeyboardMarkup::default();
    let mut row = Vec::new();
    if page > 0 {
        row.push(teloxide::types::InlineKeyboardButton::callback("上一页", format!("rank_{}", page - 1)));
    }
    if (offset + per_page) < (total as i64) {
        row.push(teloxide::types::InlineKeyboardButton::callback("下一页", format!("rank_{}", page + 1)));
    }
    if !row.is_empty() {
        keyboard.inline_keyboard.push(row);
    }

    if let Some(message_id) = message_id {
        bot.edit_message_text(chat_id, message_id, text)
            .reply_markup(keyboard)
            .await?;
    } else {
        let mut req = bot.send_message(chat_id, text).reply_markup(keyboard);
        if let Some(reply_id) = reply_to {
            req = req.reply_parameters(ReplyParameters::new(reply_id));
        }
        req.await?;
    }
    Ok(())
}

async fn callback_handler(
    bot: Bot,
    q: CallbackQuery,
    pool: SqlitePool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(data) = &q.data
        && data.starts_with("rank_") {
            let page: usize = data[5..].parse().unwrap_or(0);
            if let Some(msg) = &q.message {
                let chat_id = msg.chat().id;
                let message_id = msg.id();
                handle_rank(bot.clone(), chat_id, Some(message_id), None, pool, page).await?;
                bot.answer_callback_query(q.id).await?;
            }
        }
    Ok(())
}

async fn get_rank(pool: &SqlitePool, user_id: i64) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let row = sqlx::query(
        "SELECT COUNT(*) as rank FROM users WHERE count > (SELECT count FROM users WHERE user_id = ?) OR (count = (SELECT count FROM users WHERE user_id = ?) AND last_time < (SELECT last_time FROM users WHERE user_id = ?))"
    )
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    let rank: i64 = row.try_get("rank")?;
    Ok((rank + 1) as usize)
}