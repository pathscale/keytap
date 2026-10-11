//! Config file location and load/save.

use keytap_rules::Config;
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

/// Resolve the config path.
///
/// Order:
/// 1. `KEYTAP_CONFIG` env var (file path)
/// 2. `~/Library/Application Support/keytap/config`
pub fn config_path() -> PathBuf {
    if let Ok(p) = env::var("KEYTAP_CONFIG") {
        return PathBuf::from(p);
    }
    let home = env::var_os("HOME").expect("HOME must be set");
    PathBuf::from(home).join("Library/Application Support/keytap/config")
}

pub fn load_or_default() -> Result<(PathBuf, Config), String> {
    let path = config_path();
    if !path.exists() {
        return Ok((path, Config::default()));
    }
    let text = fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let cfg = Config::parse(&text)?;
    Ok((path, cfg))
}

pub fn write_default(path: &std::path::Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, Config::default().render())
}
