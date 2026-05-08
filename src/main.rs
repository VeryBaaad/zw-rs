use std::env;
use teloxide::{prelude::*, types::{InlineKeyboardMarkup, MessageId, ReplyParameters, InlineQuery, InlineQueryResult, InlineQueryResultArticle, InputMessageContent}, utils::command::BotCommands};
use sqlx::{SqlitePool, Row};
use chrono::{Utc, Duration};
use log::Level;

#[derive(BotCommands, Clone, Debug)]
#[command(rename_rule = "lowercase")]
enum Command {
    Zw(String),
    Rank(String),
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    log(Level::Info, "ZWBotDaemon", "Starting bot...");

    let bot = Bot::from_env();

    let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:zw.db".to_string());
    log(Level::Info, "ZWBotDaemon", &format!("Connecting to database: {}", database_url));
    let pool = SqlitePool::connect(&database_url).await.expect("Failed to connect to database");
    log(Level::Info, "ZWBotDaemon", "Database connected successfully");

    // Create table if not exists
    log(Level::Debug, "ZWBotDaemon", "Creating table if not exists");
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
    log(Level::Info, "ZWBotDaemon", "Table initialization complete");

    let handler = dptree::entry()
        .branch(Update::filter_message().filter_command::<Command>().endpoint(commands_handler))
        .branch(Update::filter_callback_query().endpoint(callback_handler))
        .branch(Update::filter_inline_query().endpoint(inline_query_handler));

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
    log(Level::Info, "commands_handler", &format!("Received command: {:?}", cmd));
    match cmd {
        Command::Zw(arg) => {
            let arg = arg.trim();
            if arg.is_empty() {
                log(Level::Debug, "commands_handler", "No argument provided for zw command, treating as empty");
                handle_zw(bot, msg, pool, None).await?
            } else {
                log(Level::Debug, "commands_handler", &format!("Received argument for zw command: '{}'", arg));
                handle_zw(bot, msg, pool, Some(arg.to_string())).await?
            }
        },
        Command::Rank(arg) => {
            let page = if arg.is_empty() {
                0
            } else {
                arg.trim()
                .parse::<usize>()
                .ok()
                .map(|p| if p > 0 { p - 1 } else { 0 })
                .unwrap_or(0)
            };
            log(Level::Debug, "commands_handler", &format!("Parsed rank page argument: {}", page));
            handle_rank(bot, msg.chat.id, None, Some(msg.id), pool, page).await?;
        }
    }
    Ok(())
}

async fn handle_zw(
    bot: Bot,
    msg: Message,
    pool: SqlitePool,
    target_arg: Option<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let user = msg.from.as_ref().unwrap();
    let initiator_id = user.id.0 as i64;
    let initiator_username = user.username.as_deref().unwrap_or("未知用户");
    let initiator_name = match user.last_name.as_deref() {
        Some(last_name) => format!("{} {}", user.first_name, last_name),
        None => user.first_name.clone(),
    };

    let now = Utc::now();
    let cd_duration = Duration::minutes(30);

    // find the target user record by id or username
    async fn find_user_record(pool: &SqlitePool, key: &str) -> Result<Option<(i64, Option<chrono::DateTime<Utc>>, String, i64)>, sqlx::Error> {
        // try to parse as user_id first
        if let Ok(id) = key.parse::<i64>() {
            if let Some(row) = sqlx::query("SELECT count, last_time, username, user_id FROM users WHERE user_id = ?")
                .bind(id)
                .fetch_optional(pool)
                .await? {
                let count: i64 = row.try_get("count")?;
                let last_time: Option<chrono::DateTime<Utc>> = row.try_get("last_time").ok();
                let username: String = row.try_get("username")?;
                let user_id: i64 = row.try_get("user_id")?;
                return Ok(Some((count, last_time, username, user_id)));
            }
            Ok(None)
        } else {
            // try to parse as username (with optional @)
            let uname = key.trim_start_matches('@');
            if let Some(row) = sqlx::query("SELECT count, last_time, username, user_id FROM users WHERE username = ?")
                .bind(uname)
                .fetch_optional(pool)
                .await? {
                let count: i64 = row.try_get("count")?;
                let last_time: Option<chrono::DateTime<Utc>> = row.try_get("last_time").ok();
                let username: String = row.try_get("username")?;
                let user_id: i64 = row.try_get("user_id")?;
                return Ok(Some((count, last_time, username, user_id)));
            }
            Ok(None)
        }
    }

    if target_arg.is_none() {
        return handle_zw_self(bot, msg, pool).await;
    }

    let target_key = target_arg.unwrap();
    let target_key = target_key.trim().trim_start_matches('@').to_string();

    let target_record = find_user_record(&pool, &target_key).await?;
    if target_record.is_none() {
        let text = format!("未找到用户 {} 的记录，无法进行帮助。", target_key);
        let _ = bot.send_message(msg.chat.id, text)
            .reply_parameters(ReplyParameters::new(msg.id))
            .await;
        return Ok(());
    }
    let (target_count, target_last_time_opt, target_username, target_user_id) = target_record.unwrap();

    let initiator_row = sqlx::query("SELECT count, last_time FROM users WHERE user_id = ?")
        .bind(initiator_id)
        .fetch_optional(&pool)
        .await?;
    let (initiator_count, initiator_last_time_opt) = if let Some(row) = initiator_row {
        let c: i64 = row.try_get("count")?;
        let lt: Option<chrono::DateTime<Utc>> = row.try_get("last_time").ok();
        (c, lt)
    } else {
        (0, None)
    };

    let mut any_in_cd = false;
    let mut cd_messages = Vec::new();

    if let Some(lt) = initiator_last_time_opt {
        let next = lt + cd_duration;
        if now < next {
            any_in_cd = true;
            let remaining = next - now;
            let mins = remaining.num_minutes();
            let secs = remaining.num_seconds() % 60;
            cd_messages.push(format!("发起者 {} 仍在冷却：{}分{}秒", initiator_name, mins, secs));
        }
    }
    if let Some(lt) = target_last_time_opt {
        let next = lt + cd_duration;
        if now < next {
            any_in_cd = true;
            let remaining = next - now;
            let mins = remaining.num_minutes();
            let secs = remaining.num_seconds() % 60;
            cd_messages.push(format!("另一位 {} 仍在冷却：{}分{}秒", target_username, mins, secs));
        }
    }

    if any_in_cd {
        let initiator_rank = get_rank(&pool, initiator_id).await.unwrap_or(0);
        let target_rank = get_rank(&pool, target_user_id).await.unwrap_or(0);
        let text = format!(
            "{}，杂鱼杂鱼，他好像昏厥了呢\n\n发起者：{}\n次数：{}次\n排行榜位置：{}\n\n另一位：{}\n次数：{}次\n排行榜位置：{}\n\n{}",
            initiator_name,
            initiator_name, initiator_count, initiator_rank,
            target_username, target_count, target_rank,
            cd_messages.join("\n")
        );
        let _ = bot.send_message(msg.chat.id, text)
            .reply_parameters(ReplyParameters::new(msg.id))
            .await;
        return Ok(());
    }

    let new_initiator_count = initiator_count + 1;
    let new_target_count = target_count + 1;

    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO users (user_id, username, count, last_time) VALUES (?, ?, ?, ?)
         ON CONFLICT(user_id) DO UPDATE SET
         username = excluded.username,
         count = excluded.count,
         last_time = excluded.last_time"
    )
    .bind(initiator_id)
    .bind(initiator_username)
    .bind(new_initiator_count)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO users (user_id, username, count, last_time) VALUES (?, ?, ?, ?)
         ON CONFLICT(user_id) DO UPDATE SET
         username = excluded.username,
         count = excluded.count,
         last_time = excluded.last_time"
    )
    .bind(target_user_id)
    .bind(&target_username)
    .bind(new_target_count)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let initiator_rank = get_rank(&pool, initiator_id).await?;
    let target_rank = get_rank(&pool, target_user_id).await?;

    let text = format!(
        "已进行双人运动！\n\n{} 带上 {} 进行了性行为！\n{} 带上 {} 进行了性行为！\n\n发起者：{}次\n另一位：{}次\n\n您在自慰排行榜上的位置：{}\n另一位在自慰排行榜上的位置：{}\n下次可进行自慰的时间：30分0秒",
        initiator_name, target_username,
        initiator_name, target_username,
        new_initiator_count, new_target_count,
        initiator_rank, target_rank
    );

    bot.send_message(msg.chat.id, text)
        .reply_parameters(ReplyParameters::new(msg.id))
        .await?;

    Ok(())
}

async fn handle_zw_self(
    bot: Bot,
    msg: Message,
    pool: SqlitePool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    log(Level::Debug, "handle_zw", "Handling zw command");
    let user = msg.from.as_ref().unwrap();
    let user_id = user.id.0 as i64;
    let username = user.username.as_deref().unwrap_or("未知用户");
    let name = match user.last_name.as_deref() {
        Some(last_name) => format!("{} {}", user.first_name, last_name),
        None => user.first_name.clone(),
    };
    log(Level::Info, "handle_zw", &format!("User: {} (ID: {}, Username: {})", name, user_id, username));

    let now = Utc::now();
    let cd_duration = Duration::minutes(30);

    // Check if user exists
    log(Level::Debug, "handle_zw", &format!("Querying user {} from database", user_id));
    let row = sqlx::query("SELECT count, last_time FROM users WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(&pool)
        .await?;
    log(Level::Debug, "handle_zw", &format!("Query result: user_exists = {}", row.is_some()));

    let (current_count, last_time) = if let Some(row) = row {
        let count: i64 = row.try_get("count")?;
        let last_time: chrono::DateTime<Utc> = row.try_get("last_time")?;
        log(Level::Debug, "handle_zw", &format!("User exists: count={}, last_time={}", count, last_time));
        (count, Some(last_time))
    } else {
        log(Level::Debug, "handle_zw", "New user, count=0, last_time=None");
        (0, None)
    };

    if let Some(last_time) = last_time {
        let next_time = last_time + cd_duration;
        log(Level::Debug, "handle_zw", &format!("Checking cooldown: now={}, next_time={}", now, next_time));
        if now < next_time {
            log(Level::Warn, "handle_zw", &format!("User {} still in cooldown", user_id));
            let remaining = next_time - now;
            let mins = remaining.num_minutes();
            let secs = remaining.num_seconds() % 60;
            log(Level::Debug, "handle_zw", &format!("Remaining cooldown: {}m{}s", mins, secs));
            let rank = get_rank(&pool, user_id).await?;
            let text = format!(
                "{}，杂鱼杂鱼，已经达到顶峰了呢~\n\n您在自慰排行榜上的位置：{}\n总次数：{}次\n下次可进行自慰的时间：{}分{}秒",
                name, rank, current_count, mins, secs
            );
            if let Err(e) = bot.send_message(msg.chat.id, text)
                .reply_parameters(ReplyParameters::new(msg.id))
                .await {
                log(Level::Error, "handle_zw", &format!("Failed to send cooldown message: {}", e));
                return Err(Box::new(e));
            }
            return Ok(());
        }
        log(Level::Debug, "handle_zw", "Cooldown period expired, proceeding");
    } else {
        log(Level::Debug, "handle_zw", "No previous record, first time user");
    }

    // Update count and last_time
    let new_count = current_count + 1;
    log(Level::Info, "handle_zw", &format!("Updating user count: {} -> {}", current_count, new_count));
    log(Level::Debug, "handle_zw", "Inserting/updating user in database");
    if let Err(e) = sqlx::query(
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
    .await {
        log(Level::Error, "handle_zw", &format!("Failed to update user in database: {}", e));
        return Err(e.into());
    }
    log(Level::Debug, "handle_zw", "Database update successful");

    let rank = get_rank(&pool, user_id).await?;
    let text = format!(
        "已开始自慰！\n\n您在自慰排行榜上的位置：{}\n总次数：{}次\n下次可进行自慰的时间：30分0秒",
        rank, new_count
    );
    if let Err(e) = bot.send_message(msg.chat.id, text)
        .reply_parameters(ReplyParameters::new(msg.id))
        .await {
        log(Level::Error, "handle_zw", &format!("Failed to send success message: {}", e));
        return Err(Box::new(e));
    }
    log(Level::Info, "handle_zw", &format!("User {} completed action, new count: {}", user_id, new_count));
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
    log(Level::Info, "handle_rank", &format!("Handling rank command: page={}", page));
    let per_page: i64 = 10;

    log(Level::Debug, "handle_rank", "Querying total user count");
    let total = sqlx::query("SELECT COUNT(*) as count FROM users")
        .fetch_one(&pool)
        .await?
        .try_get::<i64, _>("count")? as usize;
    log(Level::Debug, "handle_rank", &format!("Total users in database: {}", total));

    let max_page_index = if total > 0 {
        ((total as f64 / per_page as f64).ceil() as usize) - 1
    } else {
        0
    };

    let valid_page = if page <= max_page_index { page } else { 0 };
    let offset: i64 = (valid_page as i64) * per_page;
    log(Level::Debug, "handle_rank", &format!("Fetching rankings: per_page={}, offset={}", per_page, offset));

    log(Level::Debug, "handle_rank", "Querying users from database");
    let rows = sqlx::query(
        "SELECT user_id, username, count FROM users ORDER BY count DESC, last_time ASC LIMIT ? OFFSET ?"
    )
    .bind(per_page)
    .bind(offset)
    .fetch_all(&pool)
    .await?;
    log(Level::Debug, "handle_rank", &format!("Retrieved {} users from database", rows.len()));

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
    if valid_page > 0 {
        row.push(teloxide::types::InlineKeyboardButton::callback("上一页", format!("rank_{}", valid_page - 1)));
    }
    if (valid_page + 1) * (per_page as usize) < total {
        row.push(teloxide::types::InlineKeyboardButton::callback("下一页", format!("rank_{}", valid_page + 1)));
    }
    if !row.is_empty() {
        keyboard.inline_keyboard.push(row);
    }

    if let Some(message_id) = message_id {
        log(Level::Debug, "handle_rank", "Editing existing rank message");
        if let Err(e) = bot.edit_message_text(chat_id, message_id, text)
            .reply_markup(keyboard)
            .await {
            log(Level::Error, "handle_rank", &format!("Failed to edit rank message: {}", e));
            return Err(Box::new(e));
        }
    } else {
        log(Level::Debug, "handle_rank", "Sending new rank message");
        let mut req = bot.send_message(chat_id, text).reply_markup(keyboard);
        if let Some(reply_id) = reply_to {
            req = req.reply_parameters(ReplyParameters::new(reply_id));
        }
        if let Err(e) = req.await {
            log(Level::Error, "handle_rank", &format!("Failed to send rank message: {}", e));
            return Err(Box::new(e));
        }
    }
    log(Level::Debug, "handle_rank", "Rank message sent successfully");
    Ok(())
}

async fn callback_handler(
    bot: Bot,
    q: CallbackQuery,
    pool: SqlitePool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(data) = &q.data
        && data.starts_with("rank_") {
        log(Level::Debug, "callback_handler", "Processing rank pagination callback");
            let page: usize = data[5..].parse().unwrap_or(0);
            log(Level::Debug, "callback_handler", &format!("Parsed page number: {}", page));
            if let Some(msg) = &q.message {
                let chat_id = msg.chat().id;
                let message_id = msg.id();
                log(Level::Debug, "callback_handler", &format!("Calling handle_rank: chat_id={}, message_id={}", chat_id, message_id));
                if let Err(e) = handle_rank(bot.clone(), chat_id, Some(message_id), None, pool, page).await {
                    log(Level::Error, "callback_handler", &format!("handle_rank failed: {}", e));
                    return Err(e);
                }
                if let Err(e) = bot.answer_callback_query(q.id).await {
                    log(Level::Error, "callback_handler", &format!("Failed to answer callback: {}", e));
                    return Err(Box::new(e));
                }
            }
        }
    Ok(())
}

async fn inline_query_handler(
    bot: Bot,
    q: InlineQuery,
    pool: SqlitePool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    log(Level::Debug, "inline_query_handler", &format!("Received inline query: '{}'", q.query));
    
    let query = q.query.trim();
    
    if query.is_empty() {
        // No parameters - show both zw and rank buttons
        log(Level::Debug, "inline_query_handler", "No parameters, showing zw and rank options");
        
        let mut results: Vec<InlineQueryResult> = Vec::new();
        
        // Zw option
        let zw_text = "点击下方按钮进行紫薇\n直接爽4！";
        let mut zw_keyboard = InlineKeyboardMarkup::default();
        zw_keyboard.inline_keyboard.push(vec![
            teloxide::types::InlineKeyboardButton::callback("自慰", "zw_self"),
            teloxide::types::InlineKeyboardButton::callback("排行榜", "rank_0"),
        ]);
        
        let zw_article = InlineQueryResultArticle::new(
            "zw",
            "自慰",
            InputMessageContent::Text(teloxide::types::InputMessageContentText {
                message_text: zw_text.to_string(),
                parse_mode: None,
                entities: None,
                link_preview_options: None,
            }),
        )
        .description("30分钟进行一次")
        .reply_markup(zw_keyboard);
        
        results.push(InlineQueryResult::Article(zw_article));
        
        // Rank option
        let rank_text = "！？排行榜？！";
        let rank_keyboard = get_rank_keyboard(&pool, 0).await.unwrap_or_default();
        
        let rank_article = InlineQueryResultArticle::new(
            "rank",
            "排行榜",
            InputMessageContent::Text(teloxide::types::InputMessageContentText {
                message_text: rank_text.to_string(),
                parse_mode: None,
                entities: None,
                link_preview_options: None,
            }),
        )
        .description("谁更多")
        .reply_markup(rank_keyboard);
        
        results.push(InlineQueryResult::Article(rank_article));
        
        if let Err(e) = bot.answer_inline_query(q.id, results).await {
            log(Level::Error, "inline_query_handler", &format!("Failed to answer inline query: {}", e));
            return Err(Box::new(e));
        }
    } else {
        // Parameters provided - parse and validate
        log(Level::Debug, "inline_query_handler", &format!("Parameters provided: '{}'", query));
        
        let parts: Vec<&str> = query.split_whitespace().collect();
        let mut results: Vec<InlineQueryResult> = Vec::new();
        
        // Try to interpret as userid for zw
        if let Ok(user_id) = parts[0].parse::<i64>() {
            if user_exists(&pool, user_id).await? {
                let zw_text = "点击下方按钮进行紫薇\n直接爽4！";
                let mut zw_keyboard = InlineKeyboardMarkup::default();
                zw_keyboard.inline_keyboard.push(vec![
                    teloxide::types::InlineKeyboardButton::callback("自慰", format!("zw_user_{}", user_id)),
                    teloxide::types::InlineKeyboardButton::callback("排行榜", "rank_0"),
                ]);
                
                let zw_article = InlineQueryResultArticle::new(
                    "zw",
                    "自慰",
                    InputMessageContent::Text(teloxide::types::InputMessageContentText {
                        message_text: zw_text.to_string(),
                        parse_mode: None,
                        entities: None,
                        link_preview_options: None,
                    }),
                )
                .description("30分钟进行一次")
                .reply_markup(zw_keyboard);
                
                results.push(InlineQueryResult::Article(zw_article));
                
                log(Level::Debug, "inline_query_handler", &format!("User {} exists, showing zw option", user_id));
            }
        }
        
        // Try to interpret as rank page
        if let Ok(page) = parts[0].parse::<usize>() {
            let per_page: i64 = 10;
            let total = get_total_users(&pool).await?;
            let max_page_index = if total > 0 {
                ((total as f64 / per_page as f64).ceil() as usize) - 1
            } else {
                0
            };
            
            let valid_page = if page <= max_page_index { page } else { 0 };
            
            let rank_text = "！？排行榜？！";
            let rank_keyboard = get_rank_keyboard(&pool, valid_page).await.unwrap_or_default();
            
            let rank_article = InlineQueryResultArticle::new(
                "rank",
                "排行榜",
                InputMessageContent::Text(teloxide::types::InputMessageContentText {
                    message_text: rank_text.to_string(),
                    parse_mode: None,
                    entities: None,
                    link_preview_options: None,
                }),
            )
            .description("谁更多")
            .reply_markup(rank_keyboard);
            
            results.push(InlineQueryResult::Article(rank_article));
            
            log(Level::Debug, "inline_query_handler", &format!("Valid rank page {}, showing rank option", valid_page));
        }
        
        if results.is_empty() {
            log(Level::Debug, "inline_query_handler", "No valid results for parameters");
        }
        
        if let Err(e) = bot.answer_inline_query(q.id, results).await {
            log(Level::Error, "inline_query_handler", &format!("Failed to answer inline query: {}", e));
            return Err(Box::new(e));
        }
    }
    
    Ok(())
}

async fn user_exists(pool: &SqlitePool, user_id: i64) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    log(Level::Debug, "user_exists", &format!("Checking if user {} exists", user_id));
    let row = sqlx::query("SELECT user_id FROM users WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

async fn get_total_users(pool: &SqlitePool) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
    log(Level::Debug, "get_total_users", "Fetching total user count");
    let row = sqlx::query("SELECT COUNT(*) as count FROM users")
        .fetch_one(pool)
        .await?;
    let count: i64 = row.try_get("count")?;
    Ok(count)
}

async fn get_rank_keyboard(
    pool: &SqlitePool,
    page: usize,
) -> Result<InlineKeyboardMarkup, Box<dyn std::error::Error + Send + Sync>> {
    log(Level::Debug, "get_rank_keyboard", &format!("Generating rank keyboard for page {}", page));
    
    let per_page: i64 = 10;
    let total = get_total_users(pool).await? as usize;
    
    let max_page_index = if total > 0 {
        ((total as f64 / per_page as f64).ceil() as usize) - 1
    } else {
        0
    };
    
    let valid_page = if page <= max_page_index { page } else { 0 };
    
    let mut keyboard = InlineKeyboardMarkup::default();
    let mut row = Vec::new();
    
    if valid_page > 0 {
        row.push(teloxide::types::InlineKeyboardButton::callback("上一页", format!("rank_{}", valid_page - 1)));
    }
    if (valid_page + 1) * (per_page as usize) < total {
        row.push(teloxide::types::InlineKeyboardButton::callback("下一页", format!("rank_{}", valid_page + 1)));
    }
    
    if !row.is_empty() {
        keyboard.inline_keyboard.push(row);
    }
    
    Ok(keyboard)
}

async fn get_rank(pool: &SqlitePool, user_id: i64) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    log(Level::Debug, "get_rank", &format!("Calculating rank for user: {}", user_id));
    let row = match sqlx::query(
        "SELECT COUNT(*) as rank FROM users WHERE count > (SELECT count FROM users WHERE user_id = ?) OR (count = (SELECT count FROM users WHERE user_id = ?) AND last_time < (SELECT last_time FROM users WHERE user_id = ?))"
    )
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .fetch_one(pool)
    .await {
        Ok(r) => r,
        Err(e) => {
            log(Level::Error, "get_rank", &format!("Failed to fetch rank for user {}: {}", user_id, e));
            return Err(Box::new(e));
        }
    };
    let rank: i64 = row.try_get("rank")?;
    let final_rank = (rank + 1) as usize;
    log(Level::Debug, "get_rank", &format!("User {} rank: {}", user_id, final_rank));
    Ok(final_rank)
}

fn log(priority: Level, tag: &str, msg: &str) {
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
    let output: String = format!("[ {} {}/{} ] {}", chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f"), shortlevel, tag, msg);
    log::log!(priority, "{}", output);
    println!("{}", output);
}
