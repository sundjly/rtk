# RTK 隐私数据泄漏风险审计 & TODO

**审计日期：** 2026-03-31
**审计范围：** 远程遥测、本地数据存储、命令执行链路

---

## 风险总结矩阵

| 风险类别 | 严重度 | 影响范围 | 当前缓解措施 |
|----------|--------|----------|-------------|
| tracking.db 存储完整命令参数（含密码/token） | 🔴 严重 | 所有使用 rtk 的命令 | 无 |
| tracking.db 存储 raw input/output | 🔴 严重 | read、env、curl、grep 等 | 无 |
| tee 系统存储完整原始输出 | 🔴 严重 | 所有失败命令 | 可通过 config 关闭 |
| env 掩码不完整 + raw 值入库 | 🔴 严重 | 使用 `rtk env` 时 | 部分掩码（不充分） |
| 数据库明文无加密 | 🟠 高 | 多用户系统 | 文件权限依赖 umask |
| 90 天保留期过长 | 🟠 高 | 凭据长期暴露 | 可自行删除 |
| Telemetry 伪匿名 | 🟡 中 | 设备可追踪 | 可关闭 |
| 命令执行 shell 注入 | 🟢 低 | N/A | 直接 execve，安全 |

---

## P0 — 严重（需立即修复）

### TODO-1: tracking 写入前对 original_cmd 做凭据脱敏

**文件：** `src/core/tracking.rs` (lines 369-382)

**问题：** `original_cmd` 字段直接存储用户输入的完整命令字符串，可能包含：
- `curl -H "Authorization: Bearer sk-xxx123"`
- `git clone https://github_pat_abc123@github.com/repo.git`
- `psql -U admin -p 'MyPassword123'`

**方案：** 在 `INSERT INTO commands` 前，对 `original_cmd` 进行正则脱敏：
- URL 中的 userinfo（`https://user:pass@host` → `https://***@host`）
- `Bearer <token>` → `Bearer ***`
- `-p '<password>'` / `--password=xxx` → `-p '***'`
- 常见 provider token 模式（`sk-`、`ghp_`、`github_pat_`、`xoxb-` 等）

### TODO-2: read/env/curl/grep 的 raw input 不应完整存入 tracking.db

**文件：**
- `src/cmds/system/read.rs` (line 74-79) — 文件完整内容作为 raw input
- `src/cmds/system/env_cmd.rs` (line 126-128) — 所有环境变量（含 raw 值）
- `src/cmds/cloud/curl_cmd.rs` (line 42-44) — HTTP 响应体
- `src/cmds/cloud/wget_cmd.rs` (line 42-47) — 下载内容
- `src/cmds/system/grep_cmd.rs` (line 106, 144-149) — 匹配结果

**问题：** `timer.track()` 传入的 raw input 包含敏感数据（.env 文件内容、API 响应中的 token、环境变量值等），全部明文存入 SQLite。

**方案（二选一）：**
1. tracking 仅存储 token 计数（`input_tokens`/`output_tokens`），不存储原始内容
2. 对 raw input 做通用凭据脱敏后再存储

### TODO-3: tee 系统对敏感输出做脱敏

**文件：** `src/core/tee.rs` (lines 106-140)

**问题：** 命令失败时，完整未过滤原始输出写入 `~/.local/share/rtk/tee/*.log`。可能包含 API 响应中的 access_token、数据库查询结果中的敏感数据等。

**方案：** 写入前应用与 TODO-1 相同的凭据脱敏逻辑，或至少在 tee 文件头部添加安全警告。

---

## P1 — 高优先级

### TODO-4: 增加敏感文件类型检测

**文件：** `src/cmds/system/read.rs`

**问题：** `rtk read .env`、`rtk read ~/.ssh/id_rsa`、`rtk read ~/.aws/credentials` 均可正常执行，无任何警告。

**方案：** 检测以下文件模式，输出警告或要求 `--force` 确认：
- `.env`、`.env.*`
- `.ssh/*`（除 `known_hosts`、`config`）
- `.aws/credentials`
- `.git/config`（可能含 token）
- `*.pem`、`*.key`

### TODO-5: 缩短默认保留期

**文件：** `src/core/tracking.rs` (line 65, `HISTORY_DAYS = 90`)

**问题：** 凭据在数据库中保留 90 天，暴露窗口过长。

**方案：** 默认保留期改为 7-14 天，允许用户通过 config 自定义。

### TODO-6: 完善 env 敏感模式列表

**文件：** `src/cmds/system/env_cmd.rs` (line 132-145)

**当前覆盖：** `key`, `secret`, `password`, `token`, `credential`, `auth`, `private`, `api_key`, `apikey`, `access_key`, `jwt`

**缺失的常见模式：**
- `DATABASE_URL`（通常包含 `user:pass@host`）
- `GITHUB_TOKEN`, `GH_TOKEN`
- `STRIPE_SECRET_KEY`, `STRIPE_`
- `SLACK_BOT_TOKEN`, `SLACK_`
- `TWILIO_AUTH_TOKEN`
- `ANTHROPIC_API_KEY`
- `OPENAI_API_KEY`
- `AWS_SECRET_ACCESS_KEY`（已部分覆盖但需验证）
- `DISCORD_TOKEN`
- `CERTIFICATE`, `CERT`, `SSH_KEY`

### TODO-7: 修复 mask_value() 信息泄漏

**文件：** `src/cmds/system/env_cmd.rs` (line 148-157)

**问题：** `mask_value()` 暴露前 2 位和后 2 位字符（如 `sk****23`），对 JWT（`eyJ...`）等 token 仍泄漏类型信息。

**方案：** 改为仅显示长度信息：`****(32 chars)` 或完全掩码 `****`。

---

## P2 — 中优先级

### TODO-8: 考虑 SQLite 加密

**文件：** `src/core/tracking.rs`

**问题：** `~/.local/share/rtk/tracking.db` 明文存储，依赖文件系统权限。在共享机器或备份场景下有泄漏风险。

**方案：** 评估集成 SQLCipher 或 `rusqlite` 的加密扩展。需权衡二进制体积和启动时间影响。

### TODO-9: parse_failures 表同样需要脱敏

**文件：** `src/core/tracking.rs` (lines 313-321, 410-417)

**问题：** `parse_failures` 表存储 `raw_command`，无法解析的命令也可能包含凭据。

**方案：** 与 TODO-1 使用相同的脱敏逻辑。

### TODO-10: Telemetry 改进

**文件：** `src/core/telemetry.rs`

**问题：**
- device_hash 基于 hostname + username，是伪匿名的，可被长期追踪
- 无 TLS pinning

**方案：**
- 公开文档中明确说明遥测 endpoint URL
- 考虑 TLS pinning 或至少验证 endpoint 域名
- 考虑使用差分隐私技术聚合数据

---

## 用户侧临时缓解措施

在代码修复前，用户可通过以下配置降低风险：

```toml
# ~/.config/rtk/config.toml

[tee]
enabled = false                    # 关闭 tee 系统，不保存原始输出

[hooks]
exclude_commands = ["curl", "wget", "psql", "mysql", "env"]  # 排除敏感命令的 hook 重写
```

定期清理：
```bash
rm ~/.local/share/rtk/tracking.db   # 删除追踪数据库
rm -rf ~/.local/share/rtk/tee/      # 删除 tee 日志
```

---

## 涉及文件清单

| 文件 | 相关 TODO |
|------|-----------|
| `src/core/tracking.rs` | TODO-1, TODO-2, TODO-5, TODO-8, TODO-9 |
| `src/core/tee.rs` | TODO-3 |
| `src/cmds/system/read.rs` | TODO-2, TODO-4 |
| `src/cmds/system/env_cmd.rs` | TODO-2, TODO-6, TODO-7 |
| `src/cmds/cloud/curl_cmd.rs` | TODO-2 |
| `src/cmds/cloud/wget_cmd.rs` | TODO-2 |
| `src/cmds/system/grep_cmd.rs` | TODO-2 |
| `src/core/telemetry.rs` | TODO-10 |
