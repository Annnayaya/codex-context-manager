use std::{
    env, fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use toml_edit::{DocumentMut, Item, Value};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Default,
    High,
    Max,
}

impl Preset {
    pub const ALL: [Self; 3] = [Self::Default, Self::High, Self::Max];

    pub fn name(self) -> &'static str {
        match self {
            Self::Default => "初始默认",
            Self::High => "高档位",
            Self::Max => "最高档位",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::High => "high",
            Self::Max => "max",
        }
    }

    pub fn window(self) -> i64 {
        match self {
            Self::Default => 272_000,
            Self::High => 500_000,
            Self::Max => 1_000_000,
        }
    }

    pub fn compact(self) -> i64 {
        self.window() * 9 / 10
    }

    pub fn parse(input: &str) -> Result<Self> {
        match input {
            "default" | "low" | "272k" => Ok(Self::Default),
            "high" | "500k" => Ok(Self::High),
            "max" | "1m" => Ok(Self::Max),
            _ => Err(Error(format!(
                "未知档位 {input}；可选 default、high、max。"
            ))),
        }
    }
}

pub fn format_tokens(tokens: i64) -> String {
    if tokens >= 1_000_000 && tokens % 1_000_000 == 0 {
        format!("{}M", tokens / 1_000_000)
    } else if tokens % 1_000 == 0 {
        format!("{}K", tokens / 1_000)
    } else {
        format!("{:.1}K", tokens as f64 / 1_000.0)
    }
}

pub fn default_config_path() -> PathBuf {
    let codex_root = env::var_os("CODEX_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            env::var_os("USERPROFILE")
                .or_else(|| env::var_os("HOME"))
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".codex")
        });
    codex_root.join("config.toml")
}

pub fn display_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(&path).to_owned()
    }
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub doc: DocumentMut,
    bom: bool,
    crlf: bool,
}

#[derive(Clone, Debug)]
pub struct Change {
    pub backup: Option<PathBuf>,
    pub snapshot: Snapshot,
}

impl Snapshot {
    pub fn read(path: &Path) -> Result<Self> {
        // Resolve redirected homes and file symlinks before replacing a config.
        let resolved = fs::canonicalize(path).map_err(|e| {
            Error(format!(
                "无法打开 {}：{e}。请选择已有的 Codex 配置文件。",
                display_path(path)
            ))
        })?;
        let bytes = fs::read(&resolved).map_err(|e| Error(format!("读取配置失败：{e}")))?;
        Self::from_bytes(resolved, bytes)
    }

    pub fn from_bytes(path: PathBuf, bytes: Vec<u8>) -> Result<Self> {
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| Error("配置文件必须使用 UTF-8 编码。".to_owned()))?;
        let bom = text.starts_with('\u{feff}');
        let text = text.trim_start_matches('\u{feff}');
        let doc = text
            .parse::<DocumentMut>()
            .map_err(|e| Error(format!("配置文件不是有效的 TOML，未做修改：{e}")))?;
        let crlf = text.contains("\r\n");
        let snapshot = Self {
            path,
            bytes,
            doc,
            bom,
            crlf,
        };
        snapshot.optional_number("model_context_window")?;
        snapshot.optional_number("model_auto_compact_token_limit")?;
        Ok(snapshot)
    }

    pub fn model(&self) -> &str {
        self.doc
            .get("model")
            .and_then(Item::as_str)
            .unwrap_or("未显式指定模型")
    }

    pub fn window(&self) -> Option<i64> {
        self.doc
            .get("model_context_window")
            .and_then(Item::as_integer)
    }

    pub fn compact(&self) -> Option<i64> {
        self.doc
            .get("model_auto_compact_token_limit")
            .and_then(Item::as_integer)
    }

    pub fn matching_preset(&self) -> Option<Preset> {
        Preset::ALL
            .into_iter()
            .find(|p| self.window() == Some(p.window()) && self.compact() == Some(p.compact()))
    }

    pub fn effective_percent(&self) -> Option<i64> {
        let parent = self.path.parent()?;
        let cache: serde_json::Value =
            serde_json::from_slice(&fs::read(parent.join("models_cache.json")).ok()?).ok()?;
        let models = cache.get("models")?.as_array()?;
        models
            .iter()
            .find(|m| m.get("slug").and_then(|s| s.as_str()) == Some(self.model()))?
            .get("effective_context_window_percent")?
            .as_i64()
            .filter(|n| (1..=100).contains(n))
    }

    pub fn overrides(&self) -> Vec<String> {
        let mut found = Vec::new();
        if let Some(profiles) = self.doc.get("profiles").and_then(Item::as_table_like) {
            for (name, profile) in profiles.iter() {
                if let Some(table) = profile.as_table_like()
                    && (table.contains_key("model_context_window")
                        || table.contains_key("model_auto_compact_token_limit"))
                {
                    found.push(format!("profiles.{name}"));
                }
            }
        }
        found
    }

    fn optional_number(&self, key: &str) -> Result<Option<i64>> {
        match self.doc.get(key) {
            None => Ok(None),
            Some(item) => item
                .as_integer()
                .filter(|n| *n > 0)
                .map(Some)
                .ok_or_else(|| Error(format!("{key} 必须是正整数，未做修改。"))),
        }
    }

    pub fn render(&self, preset: Preset) -> Result<Vec<u8>> {
        let mut doc = self.doc.clone();
        for (key, value) in [
            ("model_context_window", preset.window()),
            ("model_auto_compact_token_limit", preset.compact()),
        ] {
            let mut replacement = Value::from(value);
            // Keep inline comments and whitespace on existing values.
            if let Some(existing) = doc.get(key).and_then(Item::as_value) {
                *replacement.decor_mut() = existing.decor().clone();
            }
            doc[key] = Item::Value(replacement);
        }
        let mut output = doc.to_string();
        if self.crlf {
            output = output.replace("\r\n", "\n").replace('\n', "\r\n");
        }
        if self.bom {
            output.insert(0, '\u{feff}');
        }
        // Parse the final document before touching the file.
        let rendered = Self::from_bytes(self.path.clone(), output.into_bytes())?;
        if rendered.window() != Some(preset.window())
            || rendered.compact() != Some(preset.compact())
        {
            return Err(Error("配置预览校验失败，未做修改。".to_owned()));
        }
        Ok(rendered.bytes)
    }

    pub fn apply(&self, preset: Preset) -> Result<Change> {
        self.write_change(&self.render(preset)?)
    }

    fn check_unchanged(&self) -> Result<()> {
        let current = fs::read(&self.path).map_err(|e| Error(format!("重新检查配置失败：{e}")))?;
        if current != self.bytes {
            return Err(Error(
                "配置已被其他程序修改。请刷新后重新选择档位，避免覆盖新内容。".to_owned(),
            ));
        }
        Ok(())
    }

    fn write_change(&self, desired: &[u8]) -> Result<Change> {
        self.check_unchanged()?;
        if desired == self.bytes {
            return Ok(Change {
                backup: None,
                snapshot: self.clone(),
            });
        }
        Self::from_bytes(self.path.clone(), desired.to_vec())?;
        let parent = self
            .path
            .parent()
            .ok_or_else(|| Error("配置路径没有父目录。".to_owned()))?;
        let mut temp = tempfile::Builder::new()
            .prefix(".codex-context-")
            .suffix(".tmp")
            .tempfile_in(parent)
            .map_err(|e| Error(format!("创建临时配置失败：{e}")))?;
        temp.write_all(desired)
            .map_err(|e| Error(format!("写入临时配置失败：{e}")))?;
        temp.as_file()
            .sync_all()
            .map_err(|e| Error(format!("保存临时配置失败：{e}")))?;
        let permissions = fs::metadata(&self.path)
            .map_err(|e| Error(format!("读取配置权限失败：{e}")))?
            .permissions();
        temp.as_file()
            .set_permissions(permissions)
            .map_err(|e| Error(format!("保留配置权限失败：{e}")))?;

        // Windows ReplaceFileW requires the temporary file's write handle to be closed.
        // TempPath still cleans up the temporary name if any later step fails.
        let temp_path = temp.into_temp_path();

        let backup = self.create_backup()?;
        self.check_unchanged()?;
        atomic_replace(&temp_path, &self.path).map_err(|e| {
            Error(format!(
                "替换配置失败：{e}。原配置已备份到 {}。",
                display_path(&backup)
            ))
        })?;
        let snapshot = Self::read(&self.path)?;
        if snapshot.bytes != desired {
            return Err(Error(format!(
                "写入后配置再次发生变化。原配置备份在 {}，请刷新检查。",
                display_path(&backup)
            )));
        }
        Ok(Change {
            backup: Some(backup),
            snapshot,
        })
    }

    pub fn backup_dir(&self) -> PathBuf {
        self.path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("context-manager-backups")
    }

    fn backup_prefix(&self) -> String {
        format!(
            "{}.context-",
            self.path.file_name().unwrap_or_default().to_string_lossy()
        )
    }

    fn create_backup(&self) -> Result<PathBuf> {
        let folder = self.backup_dir();
        fs::create_dir_all(&folder)
            .map_err(|e| Error(format!("创建备份目录失败，未做修改：{e}")))?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| Error(format!("系统时间不可用：{e}")))?
            .as_nanos();
        let prefix = format!("{}{:020}-", self.backup_prefix(), stamp);
        let mut backup = tempfile::Builder::new()
            .prefix(&prefix)
            .suffix(".bak")
            .tempfile_in(&folder)
            .map_err(|e| Error(format!("创建备份失败，未做修改：{e}")))?;
        backup
            .write_all(&self.bytes)
            .map_err(|e| Error(format!("写入备份失败，未做修改：{e}")))?;
        backup
            .as_file()
            .sync_all()
            .map_err(|e| Error(format!("保存备份失败，未做修改：{e}")))?;
        let (_, path) = backup
            .keep()
            .map_err(|e| Error(format!("保留备份失败，未做修改：{e}")))?;
        Ok(path)
    }

    pub fn latest_backup(&self) -> Result<Option<PathBuf>> {
        let folder = self.backup_dir();
        let entries = match fs::read_dir(folder) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Error(format!("读取备份目录失败：{e}"))),
        };
        let prefix = self.backup_prefix();
        let mut backups = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| Error(format!("读取备份失败：{e}")))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(&prefix)
                && name.ends_with(".bak")
                && entry
                    .file_type()
                    .map_err(|e| Error(format!("检查备份失败：{e}")))?
                    .is_file()
            {
                backups.push(entry.path());
            }
        }
        backups.sort();
        Ok(backups.pop())
    }

    pub fn restore_latest(&self) -> Result<Change> {
        let backup = self
            .latest_backup()?
            .ok_or_else(|| Error("还没有可恢复的管理工具备份。".to_owned()))?;
        let bytes = fs::read(&backup).map_err(|e| Error(format!("读取备份失败：{e}")))?;
        let old = Self::from_bytes(backup, bytes)?;
        let mut desired = self.doc.clone();
        // Undo only the two managed fields. Other settings may have changed since the backup.
        for key in ["model_context_window", "model_auto_compact_token_limit"] {
            if let Some(value) = old.doc.get(key) {
                desired[key] = value.clone();
            } else {
                desired.remove(key);
            }
        }
        let mut output = desired.to_string();
        if self.crlf {
            output = output.replace("\r\n", "\n").replace('\n', "\r\n");
        }
        if self.bom {
            output.insert(0, '\u{feff}');
        }
        self.write_change(output.as_bytes())
    }
}

#[cfg(windows)]
fn atomic_replace(source: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    // The temporary file is fully flushed; ReplaceFileW preserves the destination ACL.
    // Both paths and the null pointers remain valid for this synchronous call.
    let result = unsafe {
        ReplaceFileW(
            target.as_ptr(),
            source.as_ptr(),
            std::ptr::null(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, target: &Path) -> io::Result<()> {
    fs::rename(source, target)
}
