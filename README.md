# zw-rs

一个Telegram机器人，用于记录自慰次数。

## 功能

- `/zw`: 记录自慰次数，如果CD没到则回复失败。
- `/rank`: 显示自慰排行榜，支持分页。

## 运行

1. 设置环境变量：
   - `TELOXIDE_TOKEN`: Telegram Bot Token
   - `DATABASE_URL`: SQLite数据库URL，例如 `sqlite:zw.db`

2. 运行：
   ```bash
   cargo run
   ```

## 数据库

自动创建`users`表：
- `id`: 主键
- `user_id`: 用户ID
- `username`: 用户名
- `count`: 次数
- `last_time`: 最后时间

## 注意

机器人不提供任何露骨内容，仅记录次数。

## 开源许可证

本项目使用 MIT License
