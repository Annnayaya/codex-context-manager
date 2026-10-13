#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app_icon;
mod ui;

use codex_context_manager::config::{Preset, Snapshot, default_config_path, display_path};
use std::{ffi::OsString, path::PathBuf};

#[derive(Default)]
struct Arguments {
    config: Option<PathBuf>,
    apply: Option<Preset>,
    status: bool,
    restore: bool,
    help: bool,
}

fn parse_args() -> Result<Arguments, String> {
    let mut args = std::env::args_os().skip(1);
    let mut result = Arguments::default();
    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--config") => {
                result.config = Some(PathBuf::from(
                    args.next().ok_or("--config 后需要配置文件路径。")?,
                ));
            }
            Some("--apply") => {
                let preset: OsString = args.next().ok_or("--apply 后需要档位名称。")?;
                result.apply =
                    Some(Preset::parse(&preset.to_string_lossy()).map_err(|e| e.to_string())?);
            }
            Some("--status") => result.status = true,
            Some("--restore-last") => result.restore = true,
            Some("--help" | "-h") => result.help = true,
            _ => {
                return Err(format!(
                    "未知参数 {}；使用 --help 查看用法。",
                    argument.to_string_lossy()
                ));
            }
        }
    }
    if [result.apply.is_some(), result.status, result.restore]
        .into_iter()
        .filter(|v| *v)
        .count()
        > 1
    {
        return Err("--apply、--status、--restore-last 每次只能使用一个。".to_owned());
    }
    Ok(result)
}

fn attach_console() {
    #[cfg(windows)]
    // Attach only for CLI usage; redirected handles continue to work if attachment fails.
    unsafe {
        windows_sys::Win32::System::Console::AttachConsole(
            windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS,
        );
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let path = args.config.unwrap_or_else(default_config_path);
    if args.help || args.status || args.restore || args.apply.is_some() {
        attach_console();
        if args.help {
            println!(
                "Codex Context Manager\n\n双击程序打开桌面界面。\n\n  --config <config.toml>   指定配置文件\n  --status                 读取当前状态 (JSON)\n  --apply default|high|max  应用 272K / 500K / 1M 档位\n  --restore-last           撤销最近一次切换\n  --help                   显示帮助\n\n切换时自动备份；撤销只恢复两个上下文字段。重启 Codex 并新开对话加载配置。"
            );
            return Ok(());
        }
        let snapshot = Snapshot::read(&path).map_err(|e| e.to_string())?;
        if args.status {
            println!(
                "{}",
                serde_json::json!({
                    "config": display_path(&snapshot.path),
                    "model": snapshot.model(),
                    "model_context_window": snapshot.window(),
                    "model_auto_compact_token_limit": snapshot.compact(),
                    "preset": snapshot.matching_preset().map(Preset::id),
                    "effective_context_window_percent": snapshot.effective_percent(),
                })
            );
        } else {
            let change = if args.restore {
                snapshot.restore_latest()
            } else {
                snapshot.apply(args.apply.expect("apply argument was checked"))
            }
            .map_err(|e| e.to_string())?;
            println!(
                "{}",
                serde_json::json!({
                    "changed": change.backup.is_some(),
                    "config": display_path(&change.snapshot.path),
                    "backup": change.backup.as_deref().map(display_path),
                    "model_context_window": change.snapshot.window(),
                    "model_auto_compact_token_limit": change.snapshot.compact(),
                })
            );
        }
        return Ok(());
    }
    ui::launch(path).map_err(|e| e.to_string())
}

fn main() {
    if let Err(error) = run() {
        attach_console();
        eprintln!("{error}");
        // GUI launch failures must still be visible for users who double-click the program.
        if std::env::args_os().len() == 1 {
            rfd::MessageDialog::new()
                .set_title("Codex 上下文管理")
                .set_level(rfd::MessageLevel::Error)
                .set_description(error)
                .show();
        }
        std::process::exit(1);
    }
}
