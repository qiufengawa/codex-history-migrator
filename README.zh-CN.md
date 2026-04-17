# Codex History Migrator

<p align="center">
  <img src="assets/app-icon.svg" alt="Codex History Migrator logo" width="140" />
</p>

<p align="center">
  轻量、单文件、中文界面的 Codex 本地历史迁移与 Provider 同步工具。
</p>

<p align="center">
  <a href="./README.md">English README</a>
</p>

## 这是什么

`Codex History Migrator` 是一个基于 Rust + `egui/eframe` 的 Windows GUI 工具，目标是用一个轻量级单文件 `exe`，解决 Codex 本地聊天数据的两类常见问题：

1. 在不同机器、不同 `.codex` 目录之间导出和导入聊天历史。
2. 当 `config.toml` 中当前 `model_provider` 已变化时，把旧线程同步到当前 Provider，避免历史线程在 Codex 中“看不见”或分散到不同 Provider 名下。

## 核心功能

- 中文 GUI，适合直接给普通用户使用
- 自动识别本机 Codex 目录，优先支持 `CODEX_HOME`、`USERPROFILE`、`HOME` 与 `HOMEDRIVE` + `HOMEPATH`
- 扫描当前 `.codex` 数据目录并显示概要统计
- 导出聊天迁移包，包含：
  - `state_5.sqlite`
  - `sessions/**/*.jsonl`
  - `archived_sessions/**/*.jsonl`
  - `session_index.jsonl`
- 导入迁移包到目标 `.codex`
- 导入前可选自动备份，默认开启
- 检查当前 Provider 与线程分布
- 一键同步旧线程到当前 Provider
- 同步前可选自动备份，默认开启
- 支持恢复最近一次 Provider 同步备份
- 全程进度反馈，适合扫描、导出、导入等耗时操作
- Windows 图标已内嵌，启动时不会弹出黑色控制台窗口

## 安全与完整性

当前版本已经额外处理了几类高风险问题：

- 导入解包时会拒绝路径穿越条目，防止压缩包把文件写出目标目录
- 导出时只会打包 `.codex` 下的 `sessions/` 与 `archived_sessions/` 会话文件
- 导入前会先校验 `manifest.json`、`db/threads.sqlite`、`checksums.json`
- 导入前会校验包内文件 SHA-256，能发现迁移包或会话 payload 被篡改的情况

## 不迁移什么

为了保持工具轻量、清晰且尽量安全，本项目**不会**迁移下面这些内容：

- 登录态
- API Token / 密钥
- 桌面端账号数据库
- 插件安装状态
- MCP 注册配置
- 与聊天迁移无关的其他本地缓存

## 运行环境

- Windows 10/11
- 建议直接使用 Release 页面提供的单文件 `exe`
- 如需自行编译，需要安装 Rust stable 工具链

## 开始使用

### 直接下载

前往 [GitHub Releases](https://github.com/qiufengawa/codex-history-migrator/releases) 下载最新的单文件 Windows 可执行程序：

- `codex-history-migrator-v1.0.2-windows-x86_64.exe`

### 从源码运行

```powershell
cargo run --release
```

### 本地编译

```powershell
cargo build --release
```

编译产物默认位于：

```text
target\release\codex-history-migrator.exe
```

## 典型使用流程

### 1. 历史迁移

1. 打开工具并确认 `.codex` 目录。
2. 在“概览”页点击“扫描”。
3. 切换到“导出”页导出迁移包。
4. 在目标机器打开工具。
5. 选择迁移包并执行导入。
6. 如有需要，保留“导入前自动备份”为开启状态。

### 2. Provider 统一

1. 在“同步”页检查当前 Provider。
2. 查看线程分布是否存在旧 Provider。
3. 点击一键同步到当前 Provider。
4. 如需回滚，可恢复最近一次同步备份。

## 致谢

本项目的需求方向参考了社区项目 [`GODGOD126/codex-history-sync-tool`](https://github.com/GODGOD126/codex-history-sync-tool)，但本仓库以 Rust 重写，并围绕轻量 GUI、单文件发布、导入导出与 Provider 同步体验做了重新实现。

## 许可证

MIT
