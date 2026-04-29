use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Settings {
    pub apply_susfs: bool,
    pub apply_bbg: bool,
    pub apply_rekernel: bool,
    pub local_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub project: String,
    pub branch: String,
    pub variant: String,
    pub args: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            apply_susfs: true,
            apply_bbg: true,
            apply_rekernel: true,
            local_root: env::var_os("KOKUBAN_LOCAL_ROOT").map(PathBuf::from),
        }
    }
}

pub fn bool_value(value: &str) -> Result<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(anyhow!("Invalid boolean value: {}", value)),
    }
}

pub fn config_file() -> Result<PathBuf> {
    if let Some(path) = env::var_os("KOKUBAN_CONFIG").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }

    if let Some(path) = env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path).join("kokuban-kernel-ci/config"));
    }

    let home = env::var_os("HOME").ok_or_else(|| anyhow!("Unable to locate HOME"))?;
    Ok(PathBuf::from(home).join(".config/kokuban-kernel-ci/config"))
}

pub fn preset_dir() -> Result<PathBuf> {
    Ok(config_file()?
        .parent()
        .ok_or_else(|| anyhow!("Invalid config path"))?
        .join("presets"))
}

pub fn load_settings() -> Result<Settings> {
    let mut settings = Settings::default();
    let path = config_file()?;
    if !path.is_file() {
        return Ok(settings);
    }

    let content = fs::read_to_string(&path)?;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        match key.trim() {
            "apply_susfs" => settings.apply_susfs = bool_value(value.trim())?,
            "apply_bbg" => settings.apply_bbg = bool_value(value.trim())?,
            "apply_rekernel" => settings.apply_rekernel = bool_value(value.trim())?,
            "local_root" => {
                let value = value.trim();
                settings.local_root = if value.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(value))
                };
            }
            _ => {}
        }
    }

    Ok(settings)
}

pub fn set_config_value(key: &str, value: &str) -> Result<()> {
    let normalized = match key {
        "apply_susfs" | "apply_bbg" | "apply_rekernel" => bool_value(value)?.to_string(),
        "local_root" => value.to_string(),
        _ => return Err(anyhow!("Unknown config key: {}", key)),
    };

    let path = config_file()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut lines = Vec::new();
    let mut wrote = false;
    if path.is_file() {
        for line in fs::read_to_string(&path)?.lines() {
            if line
                .split_once('=')
                .map(|(existing_key, _)| existing_key.trim() == key)
                .unwrap_or(false)
            {
                lines.push(format!("{key}={normalized}"));
                wrote = true;
            } else {
                lines.push(line.to_string());
            }
        }
    }

    if !wrote {
        lines.push(format!("{key}={normalized}"));
    }

    fs::write(path, lines.join("\n") + "\n")?;
    Ok(())
}

pub fn preset_path(name: &str) -> Result<PathBuf> {
    if name.contains('/') || name.contains('\\') || name.trim().is_empty() {
        return Err(anyhow!("Invalid preset name: {}", name));
    }
    Ok(preset_dir()?.join(format!("{name}.json")))
}

pub fn save_preset(name: &str, preset: &Preset) -> Result<()> {
    let dir = preset_dir()?;
    fs::create_dir_all(&dir)?;
    fs::write(
        preset_path(name)?,
        serde_json::to_string_pretty(preset)? + "\n",
    )?;
    Ok(())
}

pub fn load_preset(name: &str) -> Result<Preset> {
    let path = preset_path(name)?;
    let content = fs::read_to_string(&path)
        .map_err(|err| anyhow!("Failed to read preset {}: {}", name, err))?;
    Ok(serde_json::from_str(&content)?)
}
