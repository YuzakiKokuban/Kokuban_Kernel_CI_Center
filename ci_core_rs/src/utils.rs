use anyhow::{Context, Result, anyhow};
use std::collections::HashMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::config::ProjectsMap;

pub fn get_root_dir() -> PathBuf {
    env::var("CI_CENTRAL_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

pub fn get_config_path() -> PathBuf {
    get_root_dir().join("configs/projects.json")
}

pub fn get_upstream_path() -> PathBuf {
    get_root_dir().join("configs/upstream_commits.json")
}

pub fn get_workspace_dir() -> PathBuf {
    get_root_dir().join("kernel_workspace")
}

pub fn get_template_path(name: &str) -> PathBuf {
    get_root_dir().join("templates").join(name)
}

pub fn load_projects() -> Result<ProjectsMap> {
    let path = get_config_path();
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read projects.json at {:?}", path))?;
    serde_json::from_str(&content).context("Failed to parse projects.json")
}

pub fn save_json<T: serde::Serialize>(path: &Path, data: &T) -> Result<()> {
    let content = serde_json::to_string_pretty(data)?;
    fs::write(path, content + "\n")?;
    Ok(())
}

pub fn set_github_env(key: &str, value: &str) -> Result<()> {
    if let Ok(path) = env::var("GITHUB_ENV") {
        let mut file = OpenOptions::new().append(true).create(true).open(path)?;
        writeln!(file, "{}={}", key, value)?;
    }
    Ok(())
}

pub fn run_cmd(cmd: &[&str], cwd: Option<&Path>, capture: bool) -> Result<Option<String>> {
    let mut command = Command::new(cmd[0]);
    command.args(&cmd[1..]);

    if let Some(dir) = cwd {
        command.current_dir(dir);
    }

    if capture {
        let output = command.output()?;
        if !output.status.success() {
            return Err(anyhow!(
                "Command failed: {:?} Stderr: {}",
                cmd,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ))
    } else {
        let status = command.status()?;
        if !status.success() {
            return Err(anyhow!("Command failed: {:?}", cmd));
        }
        Ok(None)
    }
}

pub fn run_cmd_with_env(
    cmd: &[&str],
    cwd: Option<&Path>,
    envs: &HashMap<String, String>,
) -> Result<()> {
    let mut command = Command::new(cmd[0]);
    command.args(&cmd[1..]);

    if let Some(dir) = cwd {
        command.current_dir(dir);
    }

    command.envs(envs);

    command.stdout(Stdio::inherit());
    command.stderr(Stdio::inherit());

    let status = command.status()?;
    if !status.success() {
        return Err(anyhow!("Command failed: {:?}", cmd));
    }
    Ok(())
}
