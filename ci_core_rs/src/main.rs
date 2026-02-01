mod config;

use anyhow::{anyhow, Context, Result};
use chrono::Local;
use clap::{Parser, Subcommand};
use config::{GlobalConfig, KsuConfigItem, ProjectConfig, ProjectsMap, KSU_CONFIG_JSON};
use std::collections::HashMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Parse {
        #[arg(long)]
        project: String,
    },
    Meta {
        #[arg(long)]
        project: String,
        #[arg(long)]
        branch: String,
    },
    Matrix {
        #[arg(long)]
        project: String,
        #[arg(long)]
        token: Option<String>,
    },
    Add {
        #[arg(long)]
        key: String,
        #[arg(long)]
        repo: String,
        #[arg(long)]
        defconfig: String,
        #[arg(long)]
        localversion: String,
        #[arg(long, default_value = "未知设备")]
        device_cn: String,
        #[arg(long, default_value = "Unknown Device")]
        device_en: String,
        #[arg(long, default_value = "https://github.com/YuzakiKokuban/AnyKernel3.git")]
        ak3_repo: String,
        #[arg(long, default_value = "master")]
        ak3_branch: String,
        #[arg(long, default_value = "Kernel")]
        zip_name: String,
        #[arg(long, default_value = "")]
        toolchain_prefix: String,
    },
    Setup {
        #[arg(long)]
        token: Option<String>,
        #[arg(long, default_value = "[skip ci] ci: Sync central CI files")]
        commit_message: String,
        #[arg(long, default_value = "both")]
        readme_language: String,
    },
    Watch,
    Update {
        #[arg(long)]
        token: String,
        #[arg(long)]
        project: String,
        #[arg(long)]
        variant: String,
        #[arg(long)]
        commit_id: String,
    },
    Notify {
        #[arg(long)]
        tag: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Parse { project } => handle_parse(&project),
        Commands::Meta { project, branch } => handle_meta(&project, &branch),
        Commands::Matrix { project, token } => handle_matrix(&project, token),
        Commands::Add { key, repo, defconfig, localversion, device_cn, device_en, ak3_repo, ak3_branch, zip_name, toolchain_prefix } => {
            handle_add(key, repo, defconfig, localversion, device_cn, device_en, ak3_repo, ak3_branch, zip_name, toolchain_prefix)
        },
        Commands::Setup { token, commit_message, readme_language } => handle_setup(token, commit_message, readme_language),
        Commands::Watch => handle_watch(),
        Commands::Update { token, project, variant, commit_id } => handle_update(token, project, variant, commit_id),
        Commands::Notify { tag } => handle_notify(tag),
    }
}

fn get_config_path() -> PathBuf {
    PathBuf::from("configs/projects.json")
}

fn get_upstream_path() -> PathBuf {
    PathBuf::from("configs/upstream_commits.json")
}

fn get_workspace_dir() -> PathBuf {
    PathBuf::from("kernel_workspace")
}

fn load_projects() -> Result<ProjectsMap> {
    let path = get_config_path();
    let content = fs::read_to_string(&path).context("Failed to read projects.json")?;
    serde_json::from_str(&content).context("Failed to parse projects.json")
}

fn save_json<T: serde::Serialize>(path: &Path, data: &T) -> Result<()> {
    let content = serde_json::to_string_pretty(data)?;
    fs::write(path, content + "\n")?;
    Ok(())
}

fn set_github_env(key: &str, value: &str) -> Result<()> {
    if let Ok(path) = env::var("GITHUB_ENV") {
        let mut file = OpenOptions::new().append(true).create(true).open(path)?;
        writeln!(file, "{}={}", key, value)?;
    }
    
    let mut file = OpenOptions::new().append(true).create(true).open("build_vars.sh")?;
    writeln!(file, "export {}='{}'", key, value)?;
    Ok(())
}

fn run_cmd(cmd: &[&str], cwd: Option<&Path>, capture: bool) -> Result<Option<String>> {
    let mut command = Command::new(cmd[0]);
    command.args(&cmd[1..]);
    
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }

    if capture {
        let output = command.output()?;
        if !output.status.success() {
            return Err(anyhow!("Command failed: {:?} Stderr: {}", cmd, String::from_utf8_lossy(&output.stderr)));
        }
        Ok(Some(String::from_utf8_lossy(&output.stdout).trim().to_string()))
    } else {
        let status = command.status()?;
        if !status.success() {
            return Err(anyhow!("Command failed: {:?}", cmd));
        }
        Ok(None)
    }
}

fn handle_parse(project_key: &str) -> Result<()> {
    let projects = load_projects()?;
    let proj_val = projects.get(project_key).ok_or_else(|| anyhow!("Project not found"))?;
    let proj: ProjectConfig = serde_json::from_value(proj_val.clone())?;

    set_github_env("PROJECT_REPO", &proj.repo)?;
    set_github_env("PROJECT_DEFCONFIG", &proj.defconfig)?;
    set_github_env("PROJECT_LOCALVERSION_BASE", &proj.localversion_base)?;
    
    if let Some(lto) = &proj.lto {
        set_github_env("PROJECT_LTO", lto)?;
    }
    
    set_github_env("PROJECT_TOOLCHAIN_PREFIX", proj.toolchain_path_prefix.as_deref().unwrap_or(""))?;
    set_github_env("PROJECT_ZIP_NAME_PREFIX", proj.zip_name_prefix.as_deref().unwrap_or("Kernel"))?;
    set_github_env("PROJECT_AK3_REPO", proj.anykernel_repo.as_deref().unwrap_or(""))?;
    set_github_env("PROJECT_AK3_BRANCH", proj.anykernel_branch.as_deref().unwrap_or(""))?;
    set_github_env("PROJECT_VERSION_METHOD", proj.version_method.as_deref().unwrap_or("param"))?;
    set_github_env("PROJECT_EXTRA_HOST_ENV", &proj.extra_host_env.unwrap_or(false).to_string())?;

    let exports = serde_json::to_string(&proj.toolchain_path_exports.unwrap_or_default())?;
    set_github_env("PROJECT_TOOLCHAIN_EXPORTS", &exports)?;
    
    let disable_security = serde_json::to_string(&proj.disable_security.unwrap_or_default())?;
    set_github_env("PROJECT_DISABLE_SECURITY", &disable_security)?;

    if let Some(urls) = proj.toolchain_urls {
        set_github_env("PROJECT_TOOLCHAIN_URLS", &serde_json::to_string(&urls)?)?;
    }

    Ok(())
}

fn handle_meta(project_key: &str, branch: &str) -> Result<()> {
    let projects = load_projects()?;
    let proj_val = projects.get(project_key).ok_or_else(|| anyhow!("Project not found"))?;
    let proj: ProjectConfig = serde_json::from_value(proj_val.clone())?;

    let zip_prefix = proj.zip_name_prefix.as_deref().unwrap_or("Kernel");
    let localversion_base = &proj.localversion_base;

    let variant_suffix = match branch {
        "main" | "lkm" => "LKM".to_string(),
        "ksu" => "KSU".to_string(),
        "mksu" => "MKSU".to_string(),
        "resukisu" | "sukisuultra" => "ReSuki".to_string(),
        _ => branch.to_uppercase(),
    };

    let date_str = Local::now().format("%Y%m%d-%H%M").to_string();
    
    let final_localversion = format!("{}-{}", localversion_base, variant_suffix);
    let release_tag = format!("{}-{}-{}", zip_prefix, variant_suffix, date_str);
    let final_zip_name = format!("{}-{}-{}.zip", zip_prefix, variant_suffix, date_str);
    let release_title = format!("{} {} Build ({})", zip_prefix, variant_suffix, date_str);

    set_github_env("BUILD_VARIANT_SUFFIX", &variant_suffix)?;
    set_github_env("FINAL_LOCALVERSION", &final_localversion)?;
    set_github_env("RELEASE_TAG", &release_tag)?;
    set_github_env("FINAL_ZIP_NAME", &final_zip_name)?;
    set_github_env("RELEASE_TITLE", &release_title)?;

    Ok(())
}

fn handle_matrix(project_key: &str, _token: Option<String>) -> Result<()> {
    let projects = load_projects()?;
    let proj_val = projects.get(project_key).ok_or_else(|| anyhow!("Project not found"))?;
    let proj: ProjectConfig = serde_json::from_value(proj_val.clone())?;

    let raw_supported = proj.supported_ksu.unwrap_or_default();
    let mut branches = vec!["main".to_string()];
    
    for x in raw_supported {
        if x != "sukisuultra" {
            branches.push(x);
        } else {
            branches.push("resukisu".to_string());
        }
    }

    let include: Vec<HashMap<String, String>> = branches.into_iter()
        .map(|b| HashMap::from([("branch".to_string(), b)]))
        .collect();
    
    let matrix = HashMap::from([("include", include)]);
    let json_matrix = serde_json::to_string(&matrix)?;

    if let Ok(path) = env::var("GITHUB_OUTPUT") {
        let mut file = OpenOptions::new().append(true).create(true).open(path)?;
        writeln!(file, "matrix={}", json_matrix)?;
    } else {
        println!("{}", json_matrix);
    }

    Ok(())
}

fn handle_add(key: String, repo: String, defconfig: String, localversion: String, device_cn: String, device_en: String, ak3_repo: String, ak3_branch: String, zip_name: String, toolchain_prefix: String) -> Result<()> {
    let mut projects = load_projects()?;
    
    let mut placeholders = HashMap::new();
    placeholders.insert("DEVICE_NAME_CN".to_string(), device_cn);
    placeholders.insert("DEVICE_NAME_EN".to_string(), device_en);

    let new_proj = ProjectConfig {
        repo,
        defconfig,
        localversion_base: localversion,
        lto: None,
        supported_ksu: Some(vec!["resukisu".to_string(), "mksu".to_string(), "ksu".to_string()]),
        toolchain_urls: None,
        toolchain_path_prefix: if toolchain_prefix.is_empty() { None } else { Some(toolchain_prefix) },
        toolchain_path_exports: None,
        anykernel_repo: Some(ak3_repo),
        anykernel_branch: Some(ak3_branch),
        zip_name_prefix: Some(zip_name),
        version_method: None,
        extra_host_env: None,
        disable_security: None,
        readme_placeholders: Some(placeholders),
    };

    projects.insert(key, serde_json::to_value(new_proj)?);
    save_json(&get_config_path(), &projects)?;
    Ok(())
}

fn handle_setup(token: Option<String>, commit_message: String, readme_language: String) -> Result<()> {
    let projects = load_projects()?;
    let workspace = get_workspace_dir();
    
    if !workspace.exists() {
        fs::create_dir_all(&workspace)?;
    }

    let readme_tpl = fs::read_to_string("templates/README.md.tpl")?;
    let trigger_tpl = fs::read_to_string("templates/trigger-central-build.yml.tpl")?;

    run_cmd(&["git", "config", "--global", "user.name", "Kokuban-Bot"], None, false)?;
    run_cmd(&["git", "config", "--global", "user.email", "bot@kokuban.dev"], None, false)?;

    for (key, val) in projects {
        if key.starts_with("_") { continue; }
        
        let proj: ProjectConfig = serde_json::from_value(val)?;
        let repo_url = proj.repo.clone();
        
        println!("Processing project: {} -> {}", key, repo_url);
        
        let target_dir = workspace.join(&key);
        let auth_url = if let Some(t) = &token {
            format!("https://{}@github.com/{}.git", t, repo_url)
        } else {
            format!("https://github.com/{}.git", repo_url)
        };

        if target_dir.exists() {
            fs::remove_dir_all(&target_dir)?;
        }

        run_cmd(&["git", "clone", &auth_url, target_dir.to_str().unwrap()], None, false)?;

        let readme_content = process_readme(&readme_tpl, &proj, &repo_url, &readme_language);
        let target_branches = vec!["main", "ksu", "mksu", "resukisu"];

        let remote_out = run_cmd(&["git", "branch", "-r"], Some(&target_dir), true)?.unwrap_or_default();
        let remote_branches: Vec<&str> = remote_out.lines().map(|l| l.trim().trim_start_matches("origin/")).collect();

        for branch in target_branches {
            let branch_exists = remote_branches.contains(&branch);
            
            if branch == "resukisu" && !branch_exists && remote_branches.contains(&"sukisuultra") {
                run_cmd(&["git", "checkout", "sukisuultra"], Some(&target_dir), false)?;
                run_cmd(&["git", "branch", "-m", "resukisu"], Some(&target_dir), false)?;
                run_cmd(&["git", "push", "origin", "-u", "resukisu"], Some(&target_dir), false)?;
                run_cmd(&["git", "push", "origin", "--delete", "sukisuultra"], Some(&target_dir), false)?;
            } else if branch_exists {
                run_cmd(&["git", "checkout", branch], Some(&target_dir), false)?;
            } else {
                continue;
            }

            fs::write(target_dir.join("README.md"), &readme_content)?;

            let github_dir = target_dir.join(".github");
            let workflows_dir = github_dir.join("workflows");
            fs::create_dir_all(&workflows_dir)?;

            if Path::new(".github/FUNDING.yml").exists() {
                fs::copy(".github/FUNDING.yml", github_dir.join("FUNDING.yml"))?;
            }

            for old in ["build.sh", "build_kernel.sh", "update.sh", "update-kernelsu.yml"] {
                let p = target_dir.join(old);
                if p.exists() { fs::remove_file(p)?; }
            }

            let repo_owner = repo_url.split('/').next().unwrap_or("YuzakiKokuban");
            let trigger_content = trigger_tpl
                .replace("__PROJECT_KEY__", &key)
                .replace("__REPO_OWNER__", repo_owner);
            
            fs::write(workflows_dir.join("trigger-central-build.yml"), trigger_content)?;

            if Path::new("configs/universal.gitignore").exists() {
                fs::copy("configs/universal.gitignore", target_dir.join(".gitignore"))?;
            }

            run_cmd(&["git", "add", "."], Some(&target_dir), false)?;
            let status = run_cmd(&["git", "status", "--porcelain"], Some(&target_dir), true)?;
            
            if !status.unwrap_or_default().is_empty() {
                run_cmd(&["git", "commit", "-m", &format!("{} (branch: {})", commit_message, branch)], Some(&target_dir), false)?;
                run_cmd(&["git", "push", "origin", branch], Some(&target_dir), false)?;
            }
        }

        if let Some(t) = &token {
            let child = Command::new("gh")
                .args(&["secret", "set", "CI_TOKEN"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .current_dir(&target_dir)
                .spawn();
                
            if let Ok(mut child) = child {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(t.as_bytes());
                }
                let _ = child.wait();
            }
            let _ = run_cmd(&["gh", "api", "--method", "PATCH", &format!("repos/{}", repo_url), "-f", "has_sponsorships=true", "--silent"], None, false);
        }
    }
    Ok(())
}

fn process_readme(template: &str, proj: &ProjectConfig, repo_url: &str, lang: &str) -> String {
    let mut content = template.to_string();
    let placeholders = proj.readme_placeholders.clone().unwrap_or_default();
    
    let cn_name = placeholders.get("DEVICE_NAME_CN").map(|s| s.as_str()).unwrap_or("未知设备");
    let en_name = placeholders.get("DEVICE_NAME_EN").map(|s| s.as_str()).unwrap_or("Unknown Device");

    content = content.replace("__DEVICE_NAME_CN__", cn_name)
        .replace("__DEVICE_NAME_EN__", en_name)
        .replace("__PROJECT_REPO__", repo_url)
        .replace("__LOCALVERSION_BASE__", &proj.localversion_base);

    if lang == "zh-CN" {
        let re = regex::Regex::new(r"(?s).*?").unwrap();
        content = re.replace_all(&content, "").to_string();
    } else if lang == "en-US" {
        let re = regex::Regex::new(r"(?s).*?").unwrap();
        content = re.replace_all(&content, "").to_string();
    }

    content.replace("", "")
           .replace("", "")
           .replace("", "")
           .replace("", "")
           .trim()
           .to_string()
}

fn handle_watch() -> Result<()> {
    let ksu_configs: HashMap<String, KsuConfigItem> = serde_json::from_str(KSU_CONFIG_JSON)?;
    let upstream_path = get_upstream_path();
    let mut track_data: HashMap<String, String> = if upstream_path.exists() {
        serde_json::from_str(&fs::read_to_string(&upstream_path)?)?
    } else {
        HashMap::new()
    };
    
    track_data.remove("sukisuultra");
    let projects_map = load_projects()?;
    let mut update_matrix = Vec::new();

    for (variant, config) in ksu_configs {
        let output = run_cmd(&["git", "ls-remote", &config.repo, &config.branch], None, true)?;
        let latest_hash = match output {
            Some(s) => s.split_whitespace().next().unwrap_or("").to_string(),
            None => continue,
        };

        let stored_hash = track_data.get(&variant).cloned().unwrap_or_default();

        if latest_hash != stored_hash {
            track_data.insert(variant.clone(), latest_hash.clone());

            for (p_key, p_val) in &projects_map {
                if p_key.starts_with("_") { continue; }
                let p: ProjectConfig = serde_json::from_value(p_val.clone())?;
                let supported = p.supported_ksu.unwrap_or_default();
                let normalized_supported: Vec<String> = supported.into_iter().map(|x| x.replace("sukisuultra", "resukisu")).collect();
                
                if normalized_supported.contains(&variant) {
                    let mut map = HashMap::new();
                    map.insert("project".to_string(), p_key.clone());
                    map.insert("variant".to_string(), variant.clone());
                    map.insert("commit_id".to_string(), latest_hash.chars().take(7).collect());
                    update_matrix.push(map);
                }
            }
        }
    }

    save_json(&upstream_path, &track_data)?;

    if let Ok(path) = env::var("GITHUB_OUTPUT") {
        let mut file = OpenOptions::new().append(true).create(true).open(path)?;
        writeln!(file, "matrix={}", serde_json::to_string(&update_matrix)?)?;
        writeln!(file, "found_updates={}", if !update_matrix.is_empty() { "true" } else { "false" })?;
    }

    Ok(())
}

fn handle_update(token: String, project_key: String, variant: String, commit_id: String) -> Result<()> {
    let projects = load_projects()?;
    let proj_val = projects.get(&project_key).ok_or_else(|| anyhow!("Project not found"))?;
    let proj: ProjectConfig = serde_json::from_value(proj_val.clone())?;

    let normalized_variant = variant.replace("sukisuultra", "resukisu");
    let repo_url = proj.repo;
    let target_dir = PathBuf::from("temp_kernel");

    if target_dir.exists() {
        fs::remove_dir_all(&target_dir)?;
    }

    let auth_url = format!("https://{}@github.com/{}.git", token, repo_url);
    run_cmd(&["git", "clone", "--depth=1", "--branch", &normalized_variant, &auth_url, target_dir.to_str().unwrap()], None, false)?;

    fs::write(target_dir.join("KERNELSU_VERSION.txt"), &commit_id)?;

    if Path::new("configs/universal.gitignore").exists() {
        fs::copy("configs/universal.gitignore", target_dir.join(".gitignore"))?;
    }

    let ksu_configs: HashMap<String, KsuConfigItem> = serde_json::from_str(KSU_CONFIG_JSON)?;
    if let Some(cfg) = ksu_configs.get(&normalized_variant) {
        let setup_script = target_dir.join("setup.sh");
        let script_content = reqwest::blocking::get(&cfg.setup_url)?.text()?;
        fs::write(&setup_script, script_content)?;
        
        let mut args = vec!["bash", "setup.sh"];
        let setup_args_refs: Vec<&str> = cfg.setup_args.iter().map(|s| s.as_str()).collect();
        args.extend(setup_args_refs);
        
        run_cmd(&args, Some(&target_dir), false)?;
        fs::remove_file(setup_script)?;
    }

    run_cmd(&["git", "config", "user.name", "Kokuban-Bot"], Some(&target_dir), false)?;
    run_cmd(&["git", "config", "user.email", "bot@kokuban.dev"], Some(&target_dir), false)?;

    run_cmd(&["git", "add", "."], Some(&target_dir), false)?;
    let status = run_cmd(&["git", "status", "--porcelain"], Some(&target_dir), true)?;

    if !status.unwrap_or_default().is_empty() {
        run_cmd(&["git", "commit", "-m", &format!("ci: update {} to {}", normalized_variant, commit_id)], Some(&target_dir), false)?;
        run_cmd(&["git", "push"], Some(&target_dir), false)?;
    }

    fs::remove_dir_all(target_dir)?;
    Ok(())
}

fn handle_notify(tag_name: String) -> Result<()> {
    let token = env::var("TELEGRAM_BOT_TOKEN").context("Missing TELEGRAM_BOT_TOKEN")?;
    let projects = load_projects()?;
    
    let globals_val = projects.get("_globals").cloned().unwrap_or(serde_json::Value::Null);
    let globals: GlobalConfig = serde_json::from_value(globals_val).unwrap_or(GlobalConfig { broadcast_channel: None, resukisu_chat_id: None, resukisu_topic_id: None });

    let mut target_project: Option<ProjectConfig> = None;
    let mut repo_url = "Unknown/Repo".to_string();

    for (key, val) in &projects {
        if key.starts_with("_") { continue; }
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

    let output = run_cmd(&["gh", "release", "view", &tag_name, "--repo", &repo_url, "--json", "assets,body,name,url,author"], None, true)?.unwrap();
    let release_info: serde_json::Value = serde_json::from_str(&output)?;

    let author = release_info["author"]["login"].as_str().unwrap_or("YuzakiKokuban");
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

        let _ = client.post(format!("https://api.telegram.org/bot{}/sendMessage", token))
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

            run_cmd(&["gh", "release", "download", &tag_name, "--repo", &repo_url, "-p", name], None, false)?;

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
                } else { form };

                let file_content = fs::read(name)?;
                let part = reqwest::blocking::multipart::Part::bytes(file_content).file_name(name.to_owned());
                let form = form.part("document", part);

                let _ = client.post(format!("https://api.telegram.org/bot{}/sendDocument", token))
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