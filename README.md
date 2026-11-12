# File Seal

**本地文件完整性鉴定产品** — SHA-256 指纹 + 时间戳账本。证明文件未被改动，无需上传任何内容。

> 下载 → 双击 `file-seal-gui.exe` → 桌面出现应用界面 → 添加文件/图片/视频进行封印与验证。

## 产品功能

### 桌面应用（推荐）
- **仪表盘**：总封印数、独立文件数、账本状态、最近封印列表
- **封印文件**：支持任意文件（图片、视频、文档、压缩包…），可批量选择或选整个文件夹
- **验证文件**：一键检测是否与封印时字节完全一致
- **历史记录**：可过滤查看全部封印
- **独立保存路径**：账本位置可自定义（默认系统数据目录），与文件本身完全分离

### 命令行（兼容原用法）
```bash
file-seal seal ./contract.pdf -m "发给客户A"
file-seal verify ./contract.pdf
file-seal list
```

## 构建

需要较新的 Rust（建议 1.80+）。

```bash
# CLI only
cargo build --release --bin file-seal --features cli

# 完整 GUI 产品（生成桌面应用）
cargo build --release --bin file-seal-gui --features gui
# 产物：target/release/file-seal-gui   （Windows 下为 .exe）
```

Windows 用户可直接下载 Release 中的 `file-seal-gui.exe` 运行（无需安装）。

## 账本位置

默认（可改）：
- Linux/macOS: `~/.local/share/file-seal/seals.jsonl`
- Windows: `%LOCALAPPDATA%\\file-seal\\seals.jsonl`

通过应用「设置」页或环境变量 `FILE_SEAL_LEDGER` / `--ledger` 覆盖。

## 它是 / 不是

| 是 | 不是 |
|----|------|
| 本地 SHA-256 + 时间戳账本 | 区块链 / 公共时间戳服务 |
| 检测封印后任何字节变化 | 证明你是作者 |
| 离线、隐私优先 | 法律公证替代品 |
| 完整桌面产品 + CLI | 云端 DMS / 电子签名平台 |

## 快速体验

1. 运行 `file-seal-gui`
2. 进入「封印文件」→ 选择一张图片或一个视频
3. 点击「开始封印」
4. 稍后修改该文件，再去「验证文件」→ 会显示哈希不匹配

## 版本

**v0.2.0** — 完整产品：仪表盘 + GUI 封印/验证 + 独立账本路径 + 原 CLI。

## License

MIT
