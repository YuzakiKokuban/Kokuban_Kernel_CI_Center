mod admin;
mod build;
mod config;
mod local;
mod settings;
mod utils;

use anyhow::Result;
use chrono::Local;
use clap::{Parser, Subcommand};
use config::{KSU_CONFIG_JSON, KsuConfigItem, ProjectConfig};
use local::LocalBuildOptions;
use std::collections::HashMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::utils::*;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Doctor,
    Projects,
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
    Preset {
        #[command(subcommand)]
        command: PresetCommands,
    },
    Run {
        name: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra_args: Vec<String>,
    },
    Features {
        project: Option<String>,
    },
    Validate,
    Cache {
        #[command(subcommand)]
        command: CacheCommands,
    },
    Toolchain {
        #[command(subcommand)]
        command: ToolchainCommands,
    },
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
        #[arg(long)]
        ak3_config: Option<String>,
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
    Build {
        #[arg(long)]
        project: String,
        #[arg(long)]
        branch: String,
        #[arg(long, action = clap::ArgAction::Set)]
        do_release: bool,
        #[arg(long, allow_hyphen_values = true)]
        custom_localversion: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        resukisu_setup_arg: Option<String>,
        #[arg(long, action = clap::ArgAction::Set)]
        apply_susfs: bool,
        #[arg(long, action = clap::ArgAction::Set)]
        apply_bbg: bool,
        #[arg(long, action = clap::ArgAction::Set)]
        apply_rekernel: bool,
    },
    Local {
        #[arg(long)]
        project: String,
        #[arg(long, default_value = "main")]
        branch: String,
        #[arg(long, default_value = "default")]
        variant: String,
        #[arg(long, action = clap::ArgAction::Set, default_value_t = false)]
        do_release: bool,
        #[arg(long, allow_hyphen_values = true)]
        custom_localversion: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        resukisu_setup_arg: Option<String>,
        #[arg(long, action = clap::ArgAction::Set)]
        apply_susfs: Option<bool>,
        #[arg(long, action = clap::ArgAction::SetTrue)]
        with_susfs: bool,
        #[arg(long, action = clap::ArgAction::SetTrue)]
        no_susfs: bool,
        #[arg(long, action = clap::ArgAction::Set)]
        apply_bbg: Option<bool>,
        #[arg(long, action = clap::ArgAction::SetTrue)]
        with_bbg: bool,
        #[arg(long, action = clap::ArgAction::SetTrue)]
        no_bbg: bool,
        #[arg(long, action = clap::ArgAction::Set)]
        apply_rekernel: Option<bool>,
        #[arg(long, action = clap::ArgAction::SetTrue)]
        with_rekernel: bool,
        #[arg(long, action = clap::ArgAction::SetTrue)]
        no_rekernel: bool,
        #[arg(long)]
        local_root: Option<PathBuf>,
        #[arg(long, action = clap::ArgAction::SetTrue)]
        offline: bool,
        #[arg(long, action = clap::ArgAction::SetTrue)]
        no_fetch: bool,
        #[arg(long, action = clap::ArgAction::SetTrue)]
        clean: bool,
        #[arg(long, action = clap::ArgAction::SetTrue)]
        dry_run: bool,
        #[arg(long, action = clap::ArgAction::SetTrue)]
        force_lock: bool,
    },
    CollectArtifacts {
        #[arg(long, default_value = "build_artifacts")]
        artifact_dir: String,
    },
}

#[derive(Subcommand)]
enum ToolchainCommands {
    Checksums {
        project: Option<String>,
        #[arg(long)]
        local_root: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    Show,
    Path,
    Set { key: String, value: String },
}

#[derive(Subcommand)]
enum PresetCommands {
    List,
    Set {
        name: String,
        project: String,
        #[arg(default_value = "main")]
        branch: String,
        #[arg(default_value = "default")]
        variant: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Show {
        name: String,
    },
    Remove {
        name: String,
    },
}

#[derive(Subcommand)]
enum CacheCommands {
    Status {
        #[arg(long)]
        local_root: Option<PathBuf>,
    },
    Clean {
        target: String,
        project: Option<String>,
        #[arg(long)]
        local_root: Option<PathBuf>,
    },
    Prune {
        #[arg(long, default_value_t = 5)]
        keep_artifacts: usize,
        #[arg(long)]
        older_than_days: Option<i64>,
        #[arg(long)]
        local_root: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Doctor => admin::handle_doctor(),
        Commands::Projects => admin::handle_projects(),
        Commands::Config { command } => match command {
            ConfigCommands::Show => admin::handle_config_show(),
            ConfigCommands::Path => admin::handle_config_path(),
            ConfigCommands::Set { key, value } => admin::handle_config_set(key, value),
        },
        Commands::Preset { command } => match command {
            PresetCommands::List => admin::handle_preset_list(),
            PresetCommands::Set {
                name,
                project,
                branch,
                variant,
                args,
            } => admin::handle_preset_set(name, project, branch, variant, args),
            PresetCommands::Show { name } => admin::handle_preset_show(name),
            PresetCommands::Remove { name } => admin::handle_preset_remove(name),
        },
        Commands::Run { name, extra_args } => admin::handle_run_preset(name, extra_args),
        Commands::Features { project } => admin::handle_features(project),
        Commands::Validate => admin::handle_validate(),
        Commands::Cache { command } => match command {
            CacheCommands::Status { local_root } => admin::handle_cache_status(local_root),
            CacheCommands::Clean {
                target,
                project,
                local_root,
            } => admin::handle_cache_clean(target, project, local_root),
            CacheCommands::Prune {
                keep_artifacts,
                older_than_days,
                local_root,
            } => admin::handle_cache_prune(keep_artifacts, older_than_days, local_root),
        },
        Commands::Toolchain { command } => match command {
            ToolchainCommands::Checksums {
                project,
                local_root,
            } => admin::handle_toolchain_checksums(project, local_root),
        },
        Commands::Parse { project } => handle_parse(&project),
        Commands::Meta { project, branch } => handle_meta(&project, &branch),
        Commands::Matrix { project, token } => handle_matrix(&project, token),
        Commands::Add {
            key,
            repo,
            defconfig,
            localversion,
            device_cn,
            device_en,
            ak3_config,
            zip_name,
            toolchain_prefix,
        } => handle_add(
            key,
            repo,
            defconfig,
            localversion,
            device_cn,
            device_en,
            ak3_config,
            zip_name,
            toolchain_prefix,
        ),
        Commands::Setup {
            token,
            commit_message,
            readme_language,
        } => handle_setup(token, commit_message, readme_language),
        Commands::Watch => handle_watch(),
        Commands::Update {
            token,
            project,
            variant,
            commit_id,
        } => handle_update(token, project, variant, commit_id),
        Commands::Notify { tag } => utils::handle_notify(tag),
        Commands::Build {
            project,
            branch,
            do_release,
            custom_localversion,
            resukisu_setup_arg,
            apply_susfs,
            apply_bbg,
            apply_rekernel,
        } => build::handle_build(
            project,
            branch,
            do_release,
            custom_localversion,
            resukisu_setup_arg,
            apply_susfs,
            apply_bbg,
            apply_rekernel,
        ),
        Commands::Local {
            project,
            branch,
            variant,
            do_release,
            custom_localversion,
            resukisu_setup_arg,
            apply_susfs,
            with_susfs,
            no_susfs,
            apply_bbg,
            with_bbg,
            no_bbg,
            apply_rekernel,
            with_rekernel,
            no_rekernel,
            local_root,
            offline,
            no_fetch,
            clean,
            dry_run,
            force_lock,
        } => {
            let settings = settings::load_settings()?;
            let resolved_susfs = if no_susfs {
                false
            } else if with_susfs {
                true
            } else {
                apply_susfs.unwrap_or(settings.apply_susfs)
            };
            let resolved_bbg = if no_bbg {
                false
            } else if with_bbg {
                true
            } else {
                apply_bbg.unwrap_or(settings.apply_bbg)
            };
            let resolved_rekernel = if no_rekernel {
                false
            } else if with_rekernel {
                true
            } else {
                apply_rekernel.unwrap_or(settings.apply_rekernel)
            };
            local::handle_local_build(LocalBuildOptions {
                project,
                branch,
                variant,
                do_release,
                custom_localversion,
                resukisu_setup_arg,
                apply_susfs: resolved_susfs,
                apply_bbg: resolved_bbg,
                apply_rekernel: resolved_rekernel,
                local_root: local_root.or(settings.local_root),
                offline,
                no_fetch,
                clean,
                dry_run,
                force_lock,
            })
        }
        Commands::CollectArtifacts { artifact_dir } => {
            build::handle_collect_artifacts(artifact_dir)
        }
    }
}

fn handle_parse(project_key: &str) -> Result<()> {
    let proj = load_project(project_key)?;

    set_github_env("PROJECT_REPO", &proj.repo)?;
    set_github_env("PROJECT_DEFCONFIG", &proj.defconfig)?;
    set_github_env("PROJECT_LOCALVERSION_BASE", &proj.localversion_base)?;

    Ok(())
}

fn handle_meta(project_key: &str, branch: &str) -> Result<()> {
    let proj = load_project(project_key)?;

    let zip_prefix = proj.zip_name_prefix.as_deref().unwrap_or("Kernel");
    let localversion_base = &proj.localversion_base;
    let variant_suffix = variant_suffix(branch);

    let date_str = Local::now().format("%Y%m%d-%H%M").to_string();

    let final_localversion = format!("{}-{}", localversion_base, variant_suffix);
    let release_tag = format!("{}-{}-{}", zip_prefix, variant_suffix, date_str);
    let clean_localversion = final_localversion.trim_start_matches('-');
    let final_zip_name = format!("{}-{}-{}.zip", zip_prefix, clean_localversion, date_str);

    let release_title = format!("{} {} Build ({})", zip_prefix, variant_suffix, date_str);

    set_github_env("BUILD_VARIANT_SUFFIX", &variant_suffix)?;
    set_github_env("FINAL_LOCALVERSION", &final_localversion)?;
    set_github_env("RELEASE_TAG", &release_tag)?;
    set_github_env("FINAL_ZIP_NAME", &final_zip_name)?;
    set_github_env("RELEASE_TITLE", &release_title)?;

    Ok(())
}

fn handle_matrix(project_key: &str, _token: Option<String>) -> Result<()> {
    let proj = load_project(project_key)?;

    let raw_supported = proj.supported_ksu.unwrap_or_default();
    let mut include = vec![HashMap::from([
        ("branch".to_string(), "main".to_string()),
        ("ksu_variant".to_string(), "default".to_string()),
    ])];

    for variant in raw_supported {
        let normalized = normalize_variant_name(&variant);
        let entry = if normalized == "resukisu" {
            HashMap::from([
                ("branch".to_string(), "resukisu".to_string()),
                ("ksu_variant".to_string(), "default".to_string()),
            ])
        } else {
            HashMap::from([
                ("branch".to_string(), "main".to_string()),
                ("ksu_variant".to_string(), normalized),
            ])
        };
        include.push(entry);
    }

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

fn handle_add(
    key: String,
    repo: String,
    defconfig: String,
    localversion: String,
    device_cn: String,
    device_en: String,
    ak3_config: Option<String>,
    zip_name: String,
    toolchain_prefix: String,
) -> Result<()> {
    let mut projects = load_projects()?;

    let mut placeholders = HashMap::new();
    placeholders.insert("DEVICE_NAME_CN".to_string(), device_cn);
    placeholders.insert("DEVICE_NAME_EN".to_string(), device_en);

    let new_proj = ProjectConfig {
        repo,
        defconfig,
        localversion_base: localversion,
        lto: None,
        supported_ksu: Some(vec!["resukisu".to_string()]),
        toolchain_urls: None,
        toolchain_sha256: None,
        toolchain_path_prefix: if toolchain_prefix.is_empty() {
            None
        } else {
            Some(toolchain_prefix)
        },
        toolchain_path_exports: None,
        anykernel_config: ak3_config,
        zip_name_prefix: Some(zip_name),
        version_method: None,
        extra_host_env: None,
        disable_security: None,
        readme_placeholders: Some(placeholders),
        susfs: None,
        bbg: None,
        rekernel: None,
        watch_upstream_variants: None,
    };

    projects.insert(key, serde_json::to_value(new_proj)?);
    save_json(&get_config_path(), &projects)?;
    Ok(())
}

fn handle_setup(
    token: Option<String>,
    commit_message: String,
    readme_language: String,
) -> Result<()> {
    let projects = load_projects()?;
    let workspace = get_workspace_dir();

    if !workspace.exists() {
        fs::create_dir_all(&workspace)?;
    }

    let readme_tpl = fs::read_to_string(get_template_path("README.md.tpl"))?;
    let trigger_tpl = fs::read_to_string(get_template_path("trigger-central-build.yml.tpl"))?;

    run_cmd(
        &["git", "config", "--global", "user.name", "Kokuban-Bot"],
        None,
        false,
    )?;
    run_cmd(
        &["git", "config", "--global", "user.email", "bot@kokuban.dev"],
        None,
        false,
    )?;

    for (key, val) in projects {
        if key.starts_with("_") {
            continue;
        }

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

        run_cmd(
            &["git", "clone", &auth_url, target_dir.to_str().unwrap()],
            None,
            false,
        )?;

        let readme_content = process_readme(&readme_tpl, &proj, &repo_url, &readme_language);
        let target_branches = vec!["main", "resukisu"];

        let remote_out =
            run_cmd(&["git", "branch", "-r"], Some(&target_dir), true)?.unwrap_or_default();
        let remote_branches: Vec<&str> = remote_out
            .lines()
            .map(|l| l.trim().trim_start_matches("origin/"))
            .collect();

        for branch in target_branches {
            let branch_exists = remote_branches.contains(&branch);

            if branch == "resukisu" && !branch_exists && remote_branches.contains(&"sukisuultra") {
                run_cmd(
                    &["git", "checkout", "sukisuultra"],
                    Some(&target_dir),
                    false,
                )?;
                run_cmd(
                    &["git", "branch", "-m", "resukisu"],
                    Some(&target_dir),
                    false,
                )?;
                run_cmd(
                    &["git", "push", "origin", "-u", "resukisu"],
                    Some(&target_dir),
                    false,
                )?;
                run_cmd(
                    &["git", "push", "origin", "--delete", "sukisuultra"],
                    Some(&target_dir),
                    false,
                )?;
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

            for old in [
                "build.sh",
                "build_kernel.sh",
                "update.sh",
                "update-kernelsu.yml",
            ] {
                let p = target_dir.join(old);
                if p.exists() {
                    fs::remove_file(p)?;
                }
            }

            let repo_owner = repo_url.split('/').next().unwrap_or("YuzakiKokuban");
            let trigger_content = trigger_tpl
                .replace("__PROJECT_KEY__", &key)
                .replace("__REPO_OWNER__", repo_owner);

            fs::write(
                workflows_dir.join("trigger-central-build.yml"),
                trigger_content,
            )?;

            let univ_ignore = get_root_dir().join("configs/universal.gitignore");
            if univ_ignore.exists() {
                fs::copy(univ_ignore, target_dir.join(".gitignore"))?;
            }

            run_cmd(&["git", "add", "."], Some(&target_dir), false)?;
            let status = run_cmd(&["git", "status", "--porcelain"], Some(&target_dir), true)?;

            if !status.unwrap_or_default().is_empty() {
                run_cmd(
                    &[
                        "git",
                        "commit",
                        "-m",
                        &format!("{} (branch: {})", commit_message, branch),
                    ],
                    Some(&target_dir),
                    false,
                )?;
                run_cmd(&["git", "push", "origin", branch], Some(&target_dir), false)?;
            }
        }

        for legacy_branch in ["ksu", "mksu"] {
            if remote_branches.contains(&legacy_branch) {
                println!("Removing legacy branch: {}", legacy_branch);
                let _ = run_cmd(
                    &["git", "push", "origin", "--delete", legacy_branch],
                    Some(&target_dir),
                    false,
                );
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
            let _ = run_cmd(
                &[
                    "gh",
                    "api",
                    "--method",
                    "PATCH",
                    &format!("repos/{}", repo_url),
                    "-f",
                    "has_sponsorships=true",
                    "--silent",
                ],
                None,
                false,
            );
        }
    }
    Ok(())
}

fn process_readme(template: &str, proj: &ProjectConfig, repo_url: &str, lang: &str) -> String {
    let mut content = template.to_string();
    let placeholders = proj.readme_placeholders.clone().unwrap_or_default();

    let cn_name = placeholders
        .get("DEVICE_NAME_CN")
        .map(|s| s.as_str())
        .unwrap_or("未知设备");
    let en_name = placeholders
        .get("DEVICE_NAME_EN")
        .map(|s| s.as_str())
        .unwrap_or("Unknown Device");

    content = content
        .replace("__DEVICE_NAME_CN__", cn_name)
        .replace("__DEVICE_NAME_EN__", en_name)
        .replace("__PROJECT_REPO__", repo_url)
        .replace("__LOCALVERSION_BASE__", &proj.localversion_base);

    let normalize_lang = lang.to_ascii_lowercase();
    let selected = match normalize_lang.as_str() {
        "zh" | "zh-cn" => render_readme_language(&content, "ZH"),
        "en" | "en-us" => render_readme_language(&content, "EN"),
        _ => strip_readme_markers(&content),
    };

    selected.trim().to_string()
}

fn render_readme_language(content: &str, marker: &str) -> String {
    let section = extract_readme_section(content, marker).unwrap_or_default();
    let footer = content
        .split_once("<!-- END-EN -->")
        .map(|(_, tail)| tail.trim())
        .unwrap_or_default();

    if footer.is_empty() {
        section
    } else if section.is_empty() {
        footer.to_string()
    } else {
        format!("{section}\n\n{footer}")
    }
}

fn extract_readme_section(content: &str, marker: &str) -> Option<String> {
    let begin = format!("<!-- BEGIN-{marker} -->");
    let end = format!("<!-- END-{marker} -->");
    let (_, rest) = content.split_once(&begin)?;
    let (section, _) = rest.split_once(&end)?;
    Some(section.trim().to_string())
}

fn strip_readme_markers(content: &str) -> String {
    content
        .replace("<!-- BEGIN-ZH -->", "")
        .replace("<!-- END-ZH -->", "")
        .replace("<!-- BEGIN-EN -->", "")
        .replace("<!-- END-EN -->", "")
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
    track_data.remove("ksu");
    track_data.remove("mksu");
    let projects_map = load_projects()?;
    let mut update_matrix = Vec::new();

    for (variant, config) in ksu_configs {
        let output = run_cmd(
            &["git", "ls-remote", &config.repo, &config.branch],
            None,
            true,
        )?;
        let latest_hash = match output {
            Some(s) => s.split_whitespace().next().unwrap_or("").to_string(),
            None => continue,
        };

        let stored_hash = track_data.get(&variant).cloned().unwrap_or_default();

        if latest_hash != stored_hash {
            track_data.insert(variant.clone(), latest_hash.clone());

            for (p_key, p_val) in &projects_map {
                if p_key.starts_with("_") {
                    continue;
                }
                let p: ProjectConfig = serde_json::from_value(p_val.clone())?;
                let supported = p
                    .supported_ksu
                    .clone()
                    .or_else(|| p.watch_upstream_variants.clone())
                    .unwrap_or_default();
                let normalized_supported: Vec<String> = supported
                    .into_iter()
                    .map(|x| normalize_variant_name(&x))
                    .collect();

                if normalized_supported.contains(&variant) {
                    let mut map = HashMap::new();
                    map.insert("project".to_string(), p_key.clone());
                    map.insert("variant".to_string(), variant.clone());
                    map.insert(
                        "commit_id".to_string(),
                        latest_hash.chars().take(7).collect(),
                    );
                    update_matrix.push(map);
                }
            }
        }
    }

    save_json(&upstream_path, &track_data)?;

    if let Ok(path) = env::var("GITHUB_OUTPUT") {
        let mut file = OpenOptions::new().append(true).create(true).open(path)?;
        writeln!(file, "matrix={}", serde_json::to_string(&update_matrix)?)?;
        writeln!(
            file,
            "found_updates={}",
            if !update_matrix.is_empty() {
                "true"
            } else {
                "false"
            }
        )?;
    }

    Ok(())
}

fn handle_update(
    token: String,
    project_key: String,
    variant: String,
    commit_id: String,
) -> Result<()> {
    let proj = load_project(&project_key)?;

    let normalized_variant = normalize_variant_name(&variant);
    let repo_url = proj.repo;
    let target_dir = PathBuf::from("temp_kernel");

    if target_dir.exists() {
        fs::remove_dir_all(&target_dir)?;
    }

    let auth_url = format!("https://{}@github.com/{}.git", token, repo_url);
    run_cmd(
        &[
            "git",
            "clone",
            "--depth=1",
            "--branch",
            &normalized_variant,
            &auth_url,
            target_dir.to_str().unwrap(),
        ],
        None,
        false,
    )?;

    fs::write(target_dir.join("KERNELSU_VERSION.txt"), &commit_id)?;

    let univ_ignore = get_root_dir().join("configs/universal.gitignore");
    if univ_ignore.exists() {
        fs::copy(univ_ignore, target_dir.join(".gitignore"))?;
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

    run_cmd(
        &["git", "config", "user.name", "Kokuban-Bot"],
        Some(&target_dir),
        false,
    )?;
    run_cmd(
        &["git", "config", "user.email", "bot@kokuban.dev"],
        Some(&target_dir),
        false,
    )?;

    run_cmd(&["git", "add", "."], Some(&target_dir), false)?;
    let status = run_cmd(&["git", "status", "--porcelain"], Some(&target_dir), true)?;

    if !status.unwrap_or_default().is_empty() {
        run_cmd(
            &[
                "git",
                "commit",
                "-m",
                &format!("ci: update {} to {}", normalized_variant, commit_id),
            ],
            Some(&target_dir),
            false,
        )?;
        run_cmd(&["git", "push"], Some(&target_dir), false)?;
    }

    fs::remove_dir_all(target_dir)?;
    Ok(())
}
