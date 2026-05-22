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
   - `/set <userid> <count>` (管理员命令)
     - 为某个用户指定次数
   - `/reset <userid>` (管理员命令)
     - 移除某个用户的数据
2. 通过内联查询
   - `@bot <arg>`
     - 当不存在参数时，将会使用默认参数
     - 当存在userid时，将允许和其他人进行双人运动
     - 当存在页码时，将排行榜打开到指定的页码

> 如果存在删除线，则表示内容暂未完成

## 运行

1. 配置（优先读取 `config.toml`，其次回退环境变量）：
   - `bot.token`（必填）→ 回退 `TELOXIDE_TOKEN`
   - `database.url`（可选，默认 `sqlite:zw.db`）→ 回退 `DATABASE_URL`
2. `config.toml` 示例（可从 `docs/config.example.toml` 复制）：
   ```toml
   [bot]
   token = "0000000000:xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"

   [database]
   url = "sqlite:zw.db"
   ```

3. 运行方式（二选一）：
   - 源码运行：
     ```bash
     cargo run
     ```
   - 二进制运行：从 [Release](https://github.com/VeryBaaad/zw-rs/releases/latest) 下载对应的二进制文件

4. Windows 服务方式运行（可选）：
   1. 以管理员权限打开 PowerShell / CMD。
   2. 安装服务(example)：
      ```powershell
      sc.exe create zw-rs binPath= "C:\path\to\zw-rs.exe" start= auto
      ```
   3. 启动服务：
      ```powershell
      sc.exe start zw-rs
      ```
   4. 停止/删除服务：
      ```powershell
      sc.exe stop zw-rs
      sc.exe delete zw-rs
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

数据库即可自行交给程序升级

对于特殊参数
- `is_admin`: 标记是否为管理员，是管理员命令的依赖
- `is_banned`: 
  - `0`: 无事发生
  - `1`: 被封禁，无法使用任何功能
  - `2`: 被做局，在使用某些功能时会受到3s~10s的延迟

## 注意事项

> [!NOTE]
> 机器人不提供任何露骨内容，仅记录次数。

## 开源许可证

本项目使用 MIT License
