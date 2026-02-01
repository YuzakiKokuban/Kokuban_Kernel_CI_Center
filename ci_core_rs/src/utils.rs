use anyhow::{Context, Result, anyhow};
use std::collections::HashMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::config::{GlobalConfig, ProjectConfig, ProjectsMap};

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

pub fn handle_notify(tag_name: String) -> Result<()> {
    let token = env::var("TELEGRAM_BOT_TOKEN").context("Missing TELEGRAM_BOT_TOKEN")?;
    let projects = load_projects()?;

    let globals_val = projects
        .get("_globals")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let globals: GlobalConfig = serde_json::from_value(globals_val).unwrap_or(GlobalConfig {
        broadcast_channel: None,
        resukisu_chat_id: None,
        resukisu_topic_id: None,
    });

    let mut target_project: Option<ProjectConfig> = None;
    let mut repo_url = "Unknown/Repo".to_string();

    for (key, val) in &projects {
        if key.starts_with("_") {
            continue;
        }
        let p: ProjectConfig = serde_json::from_value(val.clone())?;
        let prefix = p.zip_name_prefix.as_deref().unwrap_or("Kernel");

        if tag_name.starts_with(prefix) {
            target_project = Some(p.clone());
            repo_url = p.repo;
            break;
        }
    }

    if target_project.is_none() {
        println!("No project found for tag {}", tag_name);
        return Ok(());
    }

    let mut destinations = Vec::new();
    if let Some(chan) = globals.broadcast_channel {
        destinations.push((chan, None));
    }
    if tag_name.contains("ReSuki") {
        if let Some(chat) = globals.resukisu_chat_id {
            destinations.push((chat, globals.resukisu_topic_id));
        }
    }

    if destinations.is_empty() {
        println!("No destinations.");
        return Ok(());
    }

    let output = run_cmd(
        &[
            "gh",
            "release",
            "view",
            &tag_name,
            "--repo",
            &repo_url,
            "--json",
            "assets,body,name,url,author",
        ],
        None,
        true,
    )?
    .unwrap();
    let release_info: serde_json::Value = serde_json::from_str(&output)?;

    let author = release_info["author"]["login"]
        .as_str()
        .unwrap_or("YuzakiKokuban");
    let name = release_info["name"].as_str().unwrap_or("Update");
    let url = release_info["url"].as_str().unwrap_or("");

    let msg = format!(
        "兄长大人，快看！<code>{}</code> 有新的 Release 了哦。\n\n<b>版本 (Version):</b> <code>{}</code>\n<b>标题 (Title):</b> {}\n<b>作者 (Author):</b> {}\n\n总之，快去看看吧！ <a href='{}'>点击这里跳转</a>",
        repo_url, tag_name, name, author, url
    );

    let client = reqwest::blocking::Client::new();

    for (chat_id, topic_id) in &destinations {
        let mut json_body = HashMap::new();
        json_body.insert("chat_id", serde_json::to_value(chat_id)?);
        json_body.insert("text", serde_json::to_value(&msg)?);
        json_body.insert("parse_mode", serde_json::to_value("HTML")?);
        json_body.insert("disable_web_page_preview", serde_json::to_value(true)?);

        if let Some(tid) = topic_id {
            json_body.insert("message_thread_id", serde_json::to_value(tid)?);
        }

        let _ = client
            .post(format!("https://api.telegram.org/bot{}/sendMessage", token))
            .json(&json_body)
            .send();
    }

    let assets = release_info["assets"].as_array();
    if let Some(asset_list) = assets {
        for asset in asset_list {
            let name = asset["name"].as_str().unwrap();
            let size = asset["size"].as_i64().unwrap_or(0);

            if size > 50 * 1024 * 1024 {
                println!("Skipping {} (too large)", name);
                continue;
            }

            run_cmd(
                &[
                    "gh",
                    "release",
                    "download",
                    &tag_name,
                    "--repo",
                    &repo_url,
                    "-p",
                    name,
                    "--clobber",
                ],
                None,
                false,
            )?;

            for (chat_id, topic_id) in &destinations {
                let caption = format!(
                    "兄长大人，附件来了。\n<b>仓库 (Repo):</b> <code>{}</code>\n<b>版本 (Version):</b> <code>{}</code>\n\n📄 <b>文件 (File):</b> <code>{}</code>",
                    repo_url, tag_name, name
                );

                let form = reqwest::blocking::multipart::Form::new()
                    .text("chat_id", chat_id.clone())
                    .text("caption", caption)
                    .text("parse_mode", "HTML");

                let form = if let Some(tid) = topic_id {
                    form.text("message_thread_id", tid.to_string())
                } else {
                    form
                };

                let file_content = fs::read(name)?;
                let part = reqwest::blocking::multipart::Part::bytes(file_content)
                    .file_name(name.to_owned());
                let form = form.part("document", part);

                let _ = client
                    .post(format!(
                        "https://api.telegram.org/bot{}/sendDocument",
                        token
                    ))
                    .multipart(form)
                    .send();
            }
            if Path::new(name).exists() {
                fs::remove_file(name)?;
            }
        }
    }

    Ok(())
}
