use std::{fs, process::Command};
use tempfile::tempdir;

#[test]
fn executable_switches_all_three_modes_and_restores_without_touching_real_home() {
    let folder = tempdir().unwrap();
    let path = folder.path().join("config.toml");
    let original = "model = \"gpt-6.1-sol\"\n# keep\n[desktop]\ntheme = \"dark\"\n";
    fs::write(&path, original).unwrap();
    let binary = env!("CARGO_BIN_EXE_codex-context-manager");
    for (mode, expected) in [("default", 272_000), ("high", 500_000), ("max", 1_000_000)] {
        let output = Command::new(binary)
            .args(["--config"])
            .arg(&path)
            .args(["--apply", mode])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["model_context_window"].as_i64(), Some(expected));
        assert_eq!(
            result["model_auto_compact_token_limit"].as_i64(),
            Some(expected * 9 / 10)
        );
        assert!(fs::read_to_string(&path).unwrap().contains("# keep"));
    }
    let status = Command::new(binary)
        .arg("--config")
        .arg(&path)
        .arg("--status")
        .output()
        .unwrap();
    assert!(status.status.success());
    let result: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(result["preset"], "max");
    let undo = Command::new(binary)
        .arg("--config")
        .arg(&path)
        .arg("--restore-last")
        .output()
        .unwrap();
    assert!(
        undo.status.success(),
        "{}",
        String::from_utf8_lossy(&undo.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&undo.stdout).unwrap();
    assert_eq!(result["model_context_window"], 500_000);
}

#[test]
fn invalid_mode_or_conflicting_actions_exit_without_editing_the_file() {
    let folder = tempdir().unwrap();
    let path = folder.path().join("config.toml");
    let original = "model = \"gpt-6.1-sol\"\n";
    fs::write(&path, original).unwrap();
    for args in [
        vec!["--apply", "invalid"],
        vec!["--apply", "max", "--restore-last"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_codex-context-manager"))
            .arg("--config")
            .arg(&path)
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }
}

#[test]
fn codex_home_is_used_before_the_profile_home_without_writing_on_startup() {
    let folder = tempdir().unwrap();
    let path = folder.path().join("config.toml");
    let original = "model = \"gpt-6.1-sol\"\nmodel_context_window = 500000\nmodel_auto_compact_token_limit = 450000\n";
    fs::write(&path, original).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_codex-context-manager"))
        .env("CODEX_HOME", folder.path())
        .env("USERPROFILE", folder.path().join("not-the-config-home"))
        .arg("--status")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["preset"], "high");
    assert_eq!(fs::read_to_string(path).unwrap(), original);
    assert!(!folder.path().join("context-manager-backups").exists());
}
