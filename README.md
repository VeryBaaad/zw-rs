# zw-rs

一个Telegram机器人，用于记录自慰次数。

## 功能

- 记录自慰次数，如果CD没到则回复失败。
- 显示自慰排行榜，支持分页。

### 使用

1. 通过命令
   - `/zw <userid>`
     - 当不存在参数时，将会进行自慰
     - 当userid存在，将会和他进行双人运动
   - `/rank <page>`
     - 排行榜功能，默认为第一页
   - `/version`
     - 显示bot版本
2. 通过内联查询
   - `@bot <arg>`
     - 当不存在参数时，将会使用默认参数
     - 当存在userid时，将允许和其他人进行双人运动
     - ~~当存在页码时，将排行榜打开到指定的页码~~

> 如果存在删除线，则表示内容暂未完成

## 运行

1. 设置环境变量：
   - `TELOXIDE_TOKEN`: Telegram Bot Token
   - *`DATABASE_URL`: SQLite数据库URL，例如 `sqlite:zw.db`
   - *`TELOXIDE_API_URL`: 自定义Telegram API，例如 `https://api.telegram.org`
   - *`TELOXIDE_PROXY`: 自定义Telegram代理

> *表示非必须，即通过默认配置

2. 运行：
   ```bash
   cargo run
   ```

## 数据库

创建`users`表：
- `id`: 主键
- `user_id`: 用户ID
- `username`: 用户名
- `count`: 次数
- `last_time`: 最后时间

可参考
```
CREATE TABLE IF NOT EXISTS users (
   id INTEGER PRIMARY KEY,
   user_id INTEGER UNIQUE,
   username TEXT,
   count INTEGER DEFAULT 0,
   last_time DATETIME
);
```

## 注意事项

> [!NOTE]
> 机器人不提供任何露骨内容，仅记录次数。

## 开源许可证

本项目使用 MIT License
