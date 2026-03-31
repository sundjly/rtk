# RTK 安装与使用指南

**版本：** 0.34.2 (Privacy Hardened)
**平台：** macOS (Apple Silicon / Intel), Linux (x86_64 / aarch64)

---

## 快速安装

### 方式一：从压缩包安装（推荐）

已提供预编译的压缩包 `rtk-aarch64-apple-darwin.tar.gz`（2.7MB，macOS Apple Silicon）。

```bash
# 打包
cp target/release/rtk ./rtk-aarch64-apple-darwin
# 压缩
tar czf rtk-aarch64-apple-darwin.tar.gz rtk-aarch64-apple-darwin
# 1. 解压
tar -xzf rtk-aarch64-apple-darwin.tar.gz

# 2. 安装到 PATH 目录
sudo cp rtk-aarch64-apple-darwin /usr/local/bin/rtk
chmod +x /usr/local/bin/rtk

# 3. 验证安装
rtk --version    # 应输出: rtk 0.34.2
rtk gain         # 应正常显示 token 节省统计
```

> **macOS 安全提示：** 首次运行可能提示"无法验证开发者"，请前往 **系统设置 > 隐私与安全性**，点击"仍要打开"，或执行：
> ```bash
> xattr -d com.apple.quarantine /usr/local/bin/rtk
> ```

### 方式二：从源码编译

需要 Rust 工具链（`rustup`）：

```bash
git clone https://github.com/rtk-ai/rtk.git
cd rtk
cargo build --release
# 二进制文件位于 target/release/rtk（约 5.9MB）
sudo cp target/release/rtk /usr/local/bin/
```

### 卸载

```bash
sudo rm /usr/local/bin/rtk
# macOS（配置和数据在同一目录）
rm -rf ~/Library/Application\ Support/rtk
# Linux
rm -rf ~/.config/rtk              # 配置文件
rm -rf ~/.local/share/rtk         # 追踪数据库和 tee 日志
```

---

## 基本用法

### 直接代理命令

RTK 作为命令代理，过滤输出以减少 LLM token 消耗（节省 60-90%）：

```bash
rtk git status        # 精简的 git status 输出
rtk git log -10       # 压缩的 git log
rtk cargo test        # 仅显示失败用例摘要
rtk cargo clippy      # 精简的 lint 输出
```

### Hook 自动代理模式

安装 hook 后，AI 工具（Claude Code、Gemini、Cursor 等）的命令会自动通过 RTK 代理：

```bash
rtk init              # 为当前项目安装 hook
rtk init -g           # 全局安装 hook (recommand)
rtk verify            # 验证 hook 完整性
```

安装后无需手动输入 `rtk` 前缀，AI 工具执行的 `git status` 会自动重写为 `rtk git status`。

### 查看 Token 节省

```bash
rtk gain              # 总计节省统计
rtk gain --history    # 按命令查看历史节省
rtk discover          # 分析 Claude Code 历史，发现优化机会
```

### 原始输出模式

需要完整未过滤输出时：

```bash
rtk proxy git log --oneline -20    # 不过滤，但仍记录使用指标
```

---

## 隐私安全优化

此版本经过完整的隐私安全审计，修复了所有 P0（严重）和 P1（高优先级）问题。

### 1. 凭据自动脱敏（P0）

新增 `redact.rs` 凭据脱敏引擎，在数据持久化前自动清除敏感信息。覆盖 5 类凭据模式：

| 类型 | 示例 | 脱敏结果 |
|------|------|----------|
| Bearer Token | `Bearer sk-abc123def456` | `Bearer ***` |
| URL 凭据 | `https://user:pass@host` | `https://***@host` |
| 密码参数 | `--password=secret` | `--password=***` |
| Provider Token | `ghp_ABCDEF...`, `xoxb-123...`, `AKIA...` | `ghp_***`, `xoxb-***`, `AKIA***` |
| 环境变量密钥 | `API_KEY=sk_live_123` | `API_KEY=***` |

**作用域：**
- 命令追踪记录（tracking.db 中的 `original_cmd` 和 `raw_command`）
- Tee 故障日志（`~/.local/share/rtk/tee/*.log`）

### 2. 敏感文件检测（P1）

`rtk read` 读取以下文件时会输出 stderr 警告：

- `.env`、`.env.*`（环境变量文件）
- `*.pem`、`*.key`（证书/密钥文件）
- `id_rsa`、`id_ed25519`（SSH 密钥）
- `.aws/credentials`、`.aws/config`（AWS 凭据）
- `.ssh/id_*`（SSH 密钥目录）
- `.git/config`（可能含 token）
- `.kube/config`（K8s 凭据）

### 3. 数据保留期缩短（P1）

追踪数据库保留期从 **90 天缩短至 14 天**，大幅减少凭据暴露窗口。

### 4. 环境变量掩码增强（P1）

- 敏感模式从 11 个扩展至 **30 个**，新增覆盖：
  `DATABASE_URL`、`GITHUB_TOKEN`、`GH_TOKEN`、`STRIPE_*`、`SLACK_*`、`DISCORD_*`、
  `TWILIO_*`、`SENDGRID_*`、`OPENAI_*`、`ANTHROPIC_*`、`REDIS_URL`、`MONGODB_URI`、
  `CONNECTION_STRING`、`CERTIFICATE`、`SSH_KEY` 等

- 掩码方式从暴露首尾字符改为完全隐藏：`****(32 chars)`（仅显示长度）

### 修复总览

| 编号 | 优先级 | 问题 | 状态 |
|------|--------|------|------|
| TODO-1 | P0 | tracking 命令参数含凭据明文存储 | 已修复 — 写入前自动脱敏 |
| TODO-2 | P0 | raw input 完整存入 tracking.db | 误报 — 仅存储 token 计数（整数） |
| TODO-3 | P0 | tee 故障日志含敏感输出 | 已修复 — 写入前自动脱敏 |
| TODO-4 | P1 | 无敏感文件读取警告 | 已修复 — 检测 + stderr 警告 |
| TODO-5 | P1 | 90 天保留期过长 | 已修复 — 缩短至 14 天 |
| TODO-6 | P1 | env 敏感模式覆盖不足 | 已修复 — 扩展至 30 个模式 |
| TODO-7 | P1 | mask_value 泄漏首尾字符 | 已修复 — 完全掩码 |

---

## 配置

配置文件路径：
- macOS: `~/Library/Application Support/rtk/config.toml`
- Linux: `~/.config/rtk/config.toml`

> **注意：** 配置文件默认不存在，RTK 使用内置默认值。需要自定义时手动创建：
> ```bash
> # macOS
> mkdir -p ~/Library/Application\ Support/rtk
> touch ~/Library/Application\ Support/rtk/config.toml
> # Linux
> mkdir -p ~/.config/rtk
> touch ~/.config/rtk/config.toml
> ```
> 可通过 `rtk config` 查看当前生效的完整配置（含默认值）。

### 关闭 Tee（不保存故障原始输出）

```toml
[tee]
enabled = false
```

### 手动清理数据

```bash
# macOS
rm ~/Library/Application\ Support/rtk/history.db    # 删除追踪数据库
rm -rf ~/Library/Application\ Support/rtk/tee/      # 删除 tee 故障日志
# Linux
rm ~/.local/share/rtk/history.db    # 删除追踪数据库
rm -rf ~/.local/share/rtk/tee/      # 删除 tee 故障日志
```

### 关闭遥测

```toml
[telemetry]
enabled = false
```

---

## 文件变更清单

| 文件 | 变更说明 |
|------|----------|
| `src/core/redact.rs` | **新增** — 凭据脱敏引擎（5 类正则 + 8 个测试） |
| `src/core/mod.rs` | 注册 redact 模块 |
| `src/core/tracking.rs` | 集成脱敏 + 保留期 90→14 天 |
| `src/core/tee.rs` | 集成脱敏 |
| `src/cmds/system/read.rs` | 敏感文件检测 + 警告 |
| `src/cmds/system/env_cmd.rs` | 敏感模式 11→30 + mask_value 修复 |
