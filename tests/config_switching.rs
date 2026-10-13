use codex_context_manager::config::{Preset, Snapshot};
use std::{fs, path::PathBuf};
use tempfile::{TempDir, tempdir};
use toml_edit::{DocumentMut, value};

const ORIGINAL: &str = "# Codex settings\nmodel = \"gpt-6.1-sol\" # keep my model\nmodel_reasoning_effort = \"max\"\nservice_tier = \"priority\"\n\n[desktop]\ntheme = \"dark\"\n\n[plugins.\"example@runtime\"]\nenabled = true\n\n[projects.'F:\\中文路径\\demo']\ntrust_level = \"trusted\"\n";

fn fixture(contents: impl AsRef<[u8]>) -> (TempDir, PathBuf) {
    let folder = tempdir().unwrap();
    let path = folder.path().join("config.toml");
    fs::write(&path, contents).unwrap();
    (folder, path)
}

fn without_context(mut doc: DocumentMut) -> String {
    doc.remove("model_context_window");
    doc.remove("model_auto_compact_token_limit");
    doc.to_string()
}

#[test]
fn every_mode_creates_a_byte_exact_backup_and_keeps_other_settings() {
    for preset in Preset::ALL {
        let (_folder, path) = fixture(ORIGINAL);
        let before = Snapshot::read(&path).unwrap();
        let change = before.apply(preset).unwrap();
        assert_eq!(
            fs::read(change.backup.unwrap()).unwrap(),
            ORIGINAL.as_bytes()
        );
        assert_eq!(change.snapshot.window(), Some(preset.window()));
        assert_eq!(change.snapshot.compact(), Some(preset.compact()));
        assert_eq!(
            without_context(change.snapshot.doc),
            without_context(before.doc)
        );
    }
}

#[test]
fn existing_inline_comments_and_nested_overrides_survive() {
    let original = "model = \"gpt-6.1-sol\"\nmodel_context_window = 272000 # window note\nmodel_auto_compact_token_limit = 244800 # compact note\n\n[profiles.test]\nmodel_context_window = 128000\nmodel_auto_compact_token_limit = 100000\n";
    let (_folder, path) = fixture(original);
    let result = Snapshot::read(&path).unwrap().apply(Preset::Max).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("1000000 # window note"));
    assert!(text.contains("900000 # compact note"));
    assert_eq!(
        result.snapshot.doc["profiles"]["test"]["model_context_window"].as_integer(),
        Some(128000)
    );
    assert_eq!(result.snapshot.overrides(), vec!["profiles.test"]);
}

#[test]
fn windows_line_endings_and_utf8_bom_survive_switching() {
    let original = format!("\u{feff}{}", ORIGINAL.replace('\n', "\r\n"));
    let (_folder, path) = fixture(&original);
    Snapshot::read(&path).unwrap().apply(Preset::High).unwrap();
    let bytes = fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"\xef\xbb\xbf"));
    let text = std::str::from_utf8(&bytes).unwrap();
    assert!(!text.replace("\r\n", "").contains('\n'));
    assert!(!text.contains("\r\r\n"));
    assert!(text.contains("中文路径"));
}

#[test]
fn stale_snapshot_does_not_overwrite_new_settings_or_create_backup() {
    let (_folder, path) = fixture(ORIGINAL);
    let old = Snapshot::read(&path).unwrap();
    let new = ORIGINAL.replace("\"max\"", "\"high\"");
    fs::write(&path, &new).unwrap();
    assert!(
        old.apply(Preset::Max)
            .unwrap_err()
            .to_string()
            .contains("其他程序修改")
    );
    assert_eq!(fs::read_to_string(path).unwrap(), new);
    assert!(!old.backup_dir().exists());
}

#[test]
fn malformed_or_wrong_typed_settings_are_never_modified() {
    for contents in [
        "model = [\n",
        "model_context_window = \"500000\"\n",
        "model_context_window = -1\n",
    ] {
        let (_folder, path) = fixture(contents);
        assert!(Snapshot::read(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), contents);
        assert!(
            !path
                .parent()
                .unwrap()
                .join("context-manager-backups")
                .exists()
        );
    }
}

#[test]
fn applying_an_unchanged_preset_is_a_noop() {
    let (_folder, path) = fixture(ORIGINAL);
    let first = Snapshot::read(&path).unwrap().apply(Preset::High).unwrap();
    let before = fs::read(&path).unwrap();
    let count = fs::read_dir(first.snapshot.backup_dir()).unwrap().count();
    let repeated = first.snapshot.apply(Preset::High).unwrap();
    assert!(repeated.backup.is_none());
    assert_eq!(fs::read(path).unwrap(), before);
    assert_eq!(
        fs::read_dir(repeated.snapshot.backup_dir())
            .unwrap()
            .count(),
        count
    );
}

#[test]
fn undo_restores_missing_original_context_keys_but_keeps_later_changes() {
    let (_folder, path) = fixture(ORIGINAL);
    Snapshot::read(&path).unwrap().apply(Preset::Max).unwrap();
    let mut newer = fs::read_to_string(&path)
        .unwrap()
        .parse::<DocumentMut>()
        .unwrap();
    newer["model_reasoning_effort"] = value("high");
    newer["plugins"]["example@runtime"]["enabled"] = value(false);
    fs::write(&path, newer.to_string()).unwrap();
    let before_undo = fs::read(&path).unwrap();
    let restored = Snapshot::read(&path).unwrap().restore_latest().unwrap();
    assert_eq!(restored.snapshot.window(), None);
    assert_eq!(restored.snapshot.compact(), None);
    assert_eq!(
        restored.snapshot.doc["model_reasoning_effort"].as_str(),
        Some("high")
    );
    assert_eq!(
        restored.snapshot.doc["plugins"]["example@runtime"]["enabled"].as_bool(),
        Some(false)
    );
    assert_eq!(fs::read(restored.backup.unwrap()).unwrap(), before_undo);
}

#[test]
fn successive_backups_are_unique_and_latest_undoes_the_last_switch() {
    let (_folder, path) = fixture(ORIGINAL);
    let first = Snapshot::read(&path)
        .unwrap()
        .apply(Preset::Default)
        .unwrap();
    let second = first.snapshot.apply(Preset::High).unwrap();
    assert_ne!(first.backup, second.backup);
    assert_eq!(second.snapshot.latest_backup().unwrap(), second.backup);
    let undone = second.snapshot.restore_latest().unwrap();
    assert_eq!(undone.snapshot.matching_preset(), Some(Preset::Default));
}

#[test]
fn corrupt_backup_cannot_replace_a_valid_configuration() {
    let (_folder, path) = fixture(ORIGINAL);
    let change = Snapshot::read(&path).unwrap().apply(Preset::High).unwrap();
    fs::write(change.backup.unwrap(), "[invalid").unwrap();
    let before = fs::read(&path).unwrap();
    assert!(change.snapshot.restore_latest().is_err());
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn a_backup_for_another_profile_file_is_not_restored() {
    let (folder, path) = fixture(ORIGINAL);
    let other = folder.path().join("other.config.toml");
    fs::write(&other, ORIGINAL).unwrap();
    Snapshot::read(&other).unwrap().apply(Preset::Max).unwrap();
    assert!(
        Snapshot::read(&path)
            .unwrap()
            .latest_backup()
            .unwrap()
            .is_none()
    );
}
