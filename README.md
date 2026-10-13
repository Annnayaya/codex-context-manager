# Codex 上下文管理

用 Rust 编写的 Windows 桌面小工具。双击 `dist/Codex上下文管理.exe`，在 360×190 的黑白小窗口中选择档位并应用。界面仅显示当前值、三个档位、应用按钮和操作状态。切换后重启 Codex 并新开对话加载配置。

Windows x64 免安装版：[下载 EXE](https://github.com/Annnayaya/codex-context-manager/releases/download/v0.1.0/codex-context-manager-v0.1.0-windows-x64.exe) · [下载 ZIP](https://github.com/Annnayaya/codex-context-manager/releases/download/v0.1.0/codex-context-manager-v0.1.0-windows-x64.zip) · [版本说明](https://github.com/Annnayaya/codex-context-manager/releases/tag/v0.1.0)。下载 EXE 后双击运行，无需安装 Rust。

| 档位 | 上下文总窗口 | 自动压缩阈值 | Codex 预留 5% 时的可用额度 |
| --- | ---: | ---: | ---: |
| 初始默认 | 272,000 | 244,800 | 约 258K |
| 高档位 | 500,000 | 450,000 | 约 475K |
| 最高档位 | 1,000,000 | 900,000 | 约 950K |

“默认 272K”固定使用 272,000 tokens 的总窗口，不随官方默认值变化。三个档位都明确写入 `model_context_window` 和 `model_auto_compact_token_limit`。未显式设置窗口时显示“跟随模型”；自定义窗口直接显示当前数值。上表中的可用额度按 95% 估算。

## 使用

工具优先读取 `CODEX_HOME/config.toml`，否则读取用户目录下的 `.codex/config.toml`。仅在配置无法读取时显示文件选择入口，可选择已有的配置文件。

1. 选择“默认 272K”“500K”或“1M”。
2. 点击应用按钮，等待成功提示。
3. 重启 Codex 并新开对话加载配置。

支持快捷键 `1` / `2` / `3` 选择档位，`Ctrl + Enter` 应用，`F5` 刷新。

每次修改前，将原配置完整备份到同目录的 `context-manager-backups/`，然后以临时文件替换配置；Windows 使用 `ReplaceFileW`。保留注释、UTF-8 BOM、CRLF 换行、模型、插件及其他配置。检测到配置已被其他程序修改时，必须刷新后才能切换。程序启动和查看状态不会修改 Codex 配置，也不会自动关闭 Codex。

需要撤销时，可使用命令行 `--restore-last`，仅恢复最近备份中的两个上下文字段，保留此后修改的其他设置；撤销也会产生备份。备份包含原配置中的全部内容，请按原配置的方式保管。

工具管理所选文件的顶层配置。项目 `.codex/config.toml`、其他 profile 或启动参数可能覆盖全局值；可通过命令行 `--config` 指定对应文件。配置数值不会扩展模型本身的能力：1M 档位应配合支持该窗口的模型使用。

## 构建

需要 Rust 1.95 或更新版本。GUI 使用 `eframe/egui`，TOML 编辑使用 `toml_edit`，不需要 Node.js 或 WebView。

```powershell
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
```

程序使用黑白齿轮图标，构建时自动生成多尺寸 Windows 图标并嵌入 EXE 与版本信息。Windows GNU 工具链需将包含 `windres.exe` 的 MinGW `bin` 加入 `PATH`，或通过 `WINDRES` 指定该程序。MSVC 工具链需安装 Visual Studio C++ Build Tools 和 Windows SDK，在能找到 `rc.exe` 的 Developer PowerShell 中构建，或通过 `RC` 指定该程序。发行包位于 `dist/`；若包含运行库 DLL，应与 EXE 一同复制。

运行 `./build.ps1` 可检查、编译和打包。若 crates.io 连接超时，使用 `./build.ps1 -UseMirror` 通过 rsproxy 下载相同的锁定依赖；镜像配置只作用于构建命令。

`build.ps1` 会通过 Rust 路径重映射移除二进制中的本机用户、项目和工具链路径。需要分发 EXE 时请使用该脚本构建；直接运行 `cargo build` 不会自动启用这些重映射。使用自定义 MinGW 目录时，可设置 `MINGW_BIN` 或将其 `bin` 目录加入 `PATH`。

脚本使用 `CARGO_ENCODED_RUSTFLAGS`，其优先级高于 Cargo 配置文件中的 `rustflags`。如需自定义编译选项，请通过 `RUSTFLAGS` 或 `CARGO_ENCODED_RUSTFLAGS` 传入，脚本会保留这些选项并追加路径映射。

## 提交到 Git

提交源码、测试、构建脚本、`Cargo.toml`、`Cargo.lock`、说明和两张预览图片。`.gitignore` 会排除编译目录、验证记录、发行包、实际 Codex 配置、认证文件、环境变量文件和配置备份。使用发行包时，从 `build.ps1` 生成的 ZIP 分发；不要将整个工作目录直接压缩后公开。

## 命令行

所有命令都可通过 `--config` 指定测试文件，不影响实际使用的 Codex 配置。

```powershell
.\dist\Codex上下文管理.exe --status | Out-String
.\dist\Codex上下文管理.exe --apply default | Out-String
.\dist\Codex上下文管理.exe --apply high | Out-String
.\dist\Codex上下文管理.exe --apply max | Out-String
.\dist\Codex上下文管理.exe --restore-last | Out-String
.\dist\Codex上下文管理.exe --config "D:\demo\config.toml" --apply high | Out-String
```

命令行操作输出 JSON；PowerShell 示例中的管道会等待 GUI 程序的命令行输出。双击启动不弹出控制台窗口。

## 官方依据

- [OpenAI Codex 配置参考](https://learn.chatgpt.com/docs/config-file/config-reference)：上下文窗口及自动压缩阈值配置项。
- [GPT-6.1 Sol 模型说明](https://developers.openai.com/api/docs/models/gpt-6.1-sol)：模型的实际窗口上限。
