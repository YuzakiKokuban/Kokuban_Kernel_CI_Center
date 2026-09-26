use anyhow::{Result, anyhow};
use chrono::{DateTime, Duration, Utc};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::ProjectConfig;
use crate::local::{default_local_root, ensure_local_host, sanitize_path_component};
use crate::settings::{self, Preset};
use crate::utils::{cache_file_name, file_sha256};
use crate::utils::{load_project, load_projects};

fn dir_size(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.is_file() {
        return metadata.len();
    }
    if !metadata.is_dir() {
        return 0;
    }

    let mut total = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            total += dir_size(&entry.path());
        }
    }
    total
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut size = bytes as f64;
    let mut idx = 0;
    while size >= 1024.0 && idx < UNITS.len() - 1 {
        size /= 1024.0;
        idx += 1;
    }
    if idx == 0 {
        format!("{}{}", bytes, UNITS[idx])
    } else {
        format!("{:.1}{}", size, UNITS[idx])
    }
}

fn remove_path(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if path.is_dir() && !path.is_symlink() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Reject a user-supplied path component that would resolve outside `root`.
///
/// `sanitize_path_component` already replaces separators, but `.`/`..` and absolute paths
/// survive in other call paths, so validate the component shape and confirm the resolved
/// target still sits under the cache root before anything is deleted.
fn ensure_within(root: &Path, component: &str) -> Result<()> {
    let candidate = Path::new(component);
    let mut parts = candidate.components();
    let single_segment = !candidate.is_absolute()
        && matches!(parts.next(), Some(std::path::Component::Normal(_)))
        && parts.next().is_none();

    if !single_segment {
        return Err(anyhow!(
            "Invalid project name {:?}: it must be a single path segment without separators or '.'/'..'",
            component
        ));
    }

    let root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let resolved = fs::canonicalize(root.join(component))
        .unwrap_or_else(|_| root.join(component).to_path_buf());

    if !resolved.starts_with(&root) {
        return Err(anyhow!(
            "Refusing to clean {:?}: it resolves outside the cache root {}",
            component,
            root.display()
        ));
    }

    Ok(())
}

fn project_values() -> Result<Vec<(String, ProjectConfig)>> {
    let projects = load_projects()?;
    let mut values = Vec::new();
    for (key, value) in projects {
        if key.starts_with('_') {
            continue;
        }
        values.push((key, serde_json::from_value(value)?));
    }
    values.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(values)
}

pub fn handle_doctor() -> Result<()> {
    ensure_local_host()?;
    println!("OK: local build dependencies look ready.");
    Ok(())
}

pub fn handle_projects() -> Result<()> {
    for (key, _) in project_values()? {
        println!("{key}");
    }
    Ok(())
}

pub fn handle_features(project: Option<String>) -> Result<()> {
    if let Some(project) = project {
        let proj = load_project(&project)?;
        println!("project={}", project);
        println!("repo={}", proj.repo);
        println!("defconfig={}", proj.defconfig);
        if let Some(susfs) = proj.susfs {
            println!("susfs=enabled");
            println!("susfs_repo={}", susfs.repo);
            println!("susfs_branch={}", susfs.branch);
            println!("susfs_patch={}", susfs.patch_path);
        } else {
            println!("susfs=missing");
        }
        if let Some(bbg) = proj.bbg {
            println!("bbg=enabled");
            println!(
                "bbg_setup_url={}",
                bbg.setup_url.unwrap_or_else(|| {
                    "https://github.com/vc-teahouse/Baseband-guard/raw/main/setup.sh".to_string()
                })
            );
        } else {
            println!("bbg=disabled");
        }
        println!("hybridmount=enabled");
        return Ok(());
    }

    for (key, proj) in project_values()? {
        println!(
            "{}\tsusfs={}\tbbg={}\thybridmount=enabled",
            key,
            if proj.susfs.is_some() {
                "enabled"
            } else {
                "missing"
            },
            if proj.bbg.is_some() {
                "enabled"
            } else {
                "disabled"
            }
        );
    }
    Ok(())
}

pub fn handle_validate() -> Result<()> {
    let mut errors = Vec::new();
    for (key, proj) in project_values()? {
        if proj.repo.trim().is_empty() {
            errors.push(format!("{key}: missing repo"));
        }
        if proj.defconfig.trim().is_empty() {
            errors.push(format!("{key}: missing defconfig"));
        }
        if proj.localversion_base.trim().is_empty() {
            errors.push(format!("{key}: missing localversion_base"));
        }
        if proj
            .zip_name_prefix
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            errors.push(format!("{key}: missing zip_name_prefix"));
        }
        if proj
            .anykernel_config
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            errors.push(format!("{key}: missing anykernel_config"));
        }
        if proj
            .toolchain_urls
            .as_ref()
            .map(Vec::is_empty)
            .unwrap_or(true)
        {
            errors.push(format!("{key}: missing toolchain_urls"));
        }
        if proj.susfs.is_none() {
            errors.push(format!("{key}: missing susfs"));
        }
    }

    if !errors.is_empty() {
        return Err(anyhow!(errors.join("\n")));
    }

    println!("OK: project configuration is valid.");
    Ok(())
}

fn root_or_default(local_root: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = local_root {
        return Ok(path);
    }

    let settings = settings::load_settings()?;
    settings
        .local_root
        .map(Ok)
        .unwrap_or_else(default_local_root)
}

pub fn handle_cache_status(local_root: Option<PathBuf>) -> Result<()> {
    let root = root_or_default(local_root)?;
    let items = [
        ("builds", root.join("builds")),
        ("kernel_mirrors", root.join("repos/kernels")),
        ("toolchains", root.join("downloads/toolchains")),
        ("anykernel", root.join("repos/AnyKernel3")),
        ("artifacts", root.join("artifacts")),
        ("logs", root.join("logs")),
        ("locks", root.join("locks")),
        ("sccache", root.join("sccache")),
        ("ccache", root.join("ccache")),
    ];

    println!("local_root={}", root.display());
    for (name, path) in items {
        let size = dir_size(&path);
        println!(
            "{}={}",
            name,
            if size == 0 {
                "-".to_string()
            } else {
                human_size(size)
            }
        );
    }
    Ok(())
}

pub fn handle_cache_clean(
    target: String,
    project: Option<String>,
    local_root: Option<PathBuf>,
) -> Result<()> {
    let root = root_or_default(local_root)?;
    match target.as_str() {
        "artifacts" => remove_path(&root.join("artifacts"))?,
        "logs" => remove_path(&root.join("logs"))?,
        "toolchains" => remove_path(&root.join("downloads/toolchains"))?,
        "anykernel" => remove_path(&root.join("repos/AnyKernel3"))?,
        "mirrors" => remove_path(&root.join("repos/kernels"))?,
        "builds" => remove_path(&root.join("builds"))?,
        "sccache" => remove_path(&root.join("sccache"))?,
        "ccache" => remove_path(&root.join("ccache"))?,
        "locks" => remove_path(&root.join("locks"))?,
        "project" => {
            let project =
                project.ok_or_else(|| anyhow!("cache clean project requires --project"))?;
            let project_key = sanitize_path_component(&project);
            ensure_within(&root, &project)?;
            remove_path(&root.join("builds").join(&project_key))?;
            remove_path(&root.join("artifacts").join(&project_key))?;
            remove_path(&root.join("logs").join(&project_key))?;
            remove_path(
                &root
                    .join("repos/kernels")
                    .join(format!("{}.git", project_key)),
            )?;
        }
        "all" => remove_path(&root)?,
        _ => return Err(anyhow!("Unknown cache clean target: {}", target)),
    }
    println!("Cleaned cache target: {}", target);
    Ok(())
}

pub fn handle_config_show() -> Result<()> {
    let settings = settings::load_settings()?;
    println!("config_file={}", settings::config_file()?.display());
    println!("apply_susfs={}", settings.apply_susfs);
    println!("apply_bbg={}", settings.apply_bbg);
    println!("apply_hybridmount={}", settings.apply_hybridmount);
    println!(
        "local_root={}",
        settings
            .local_root
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    );
    Ok(())
}

pub fn handle_config_path() -> Result<()> {
    println!("{}", settings::config_file()?.display());
    Ok(())
}

pub fn handle_config_set(key: String, value: String) -> Result<()> {
    settings::set_config_value(&key, &value)?;
    handle_config_show()
}

pub fn handle_preset_list() -> Result<()> {
    let dir = settings::preset_dir()?;
    if !dir.is_dir() {
        return Ok(());
    }

    let mut names = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.path().extension().and_then(|ext| ext.to_str()) == Some("json")
            && let Some(stem) = entry.path().file_stem().and_then(|stem| stem.to_str())
        {
            names.push(stem.to_string());
        }
    }
    names.sort();
    for name in names {
        println!("{name}");
    }
    Ok(())
}

pub fn handle_preset_set(
    name: String,
    project: String,
    branch: String,
    variant: String,
    args: Vec<String>,
) -> Result<()> {
    let preset = Preset {
        project,
        branch,
        variant,
        args,
    };
    settings::save_preset(&name, &preset)?;
    println!("Saved preset: {name}");
    Ok(())
}

pub fn handle_preset_show(name: String) -> Result<()> {
    let preset = settings::load_preset(&name)?;
    println!("{}", serde_json::to_string_pretty(&preset)?);
    Ok(())
}

pub fn handle_preset_remove(name: String) -> Result<()> {
    remove_path(&settings::preset_path(&name)?)?;
    println!("Removed preset: {name}");
    Ok(())
}

pub fn handle_run_preset(name: String, extra_args: Vec<String>) -> Result<()> {
    let preset = settings::load_preset(&name)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("local")
        .arg("--project")
        .arg(&preset.project)
        .arg("--branch")
        .arg(&preset.branch)
        .arg("--variant")
        .arg(&preset.variant)
        .args(&preset.args)
        .args(extra_args);

    let status = command.status()?;
    if !status.success() {
        return Err(anyhow!("Preset run failed with status {status}"));
    }
    Ok(())
}

pub fn handle_toolchain_checksums(
    project: Option<String>,
    local_root: Option<PathBuf>,
) -> Result<()> {
    let root = root_or_default(local_root)?;
    let projects = if let Some(project) = project {
        vec![(project.clone(), load_project(&project)?)]
    } else {
        project_values()?
    };

    let mut output = serde_json::Map::new();
    for (key, proj) in projects {
        let mut hashes = serde_json::Map::new();
        for url in proj.toolchain_urls.unwrap_or_default() {
            let cache_path = root
                .join("downloads/toolchains")
                .join(cache_file_name(&url)?);
            let value = if cache_path.is_file() {
                serde_json::Value::String(file_sha256(&cache_path)?)
            } else {
                serde_json::Value::Null
            };
            hashes.insert(url, value);
        }
        output.insert(key, serde_json::Value::Object(hashes));
    }

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

pub fn handle_cache_prune(
    keep_artifacts: usize,
    older_than_days: Option<i64>,
    local_root: Option<PathBuf>,
) -> Result<()> {
    let root = root_or_default(local_root)?;
    let artifacts = root.join("artifacts");
    if !artifacts.is_dir() {
        println!("No artifact cache found.");
        return Ok(());
    }

    let cutoff = older_than_days.map(|days| Utc::now() - Duration::days(days));
    for project in fs::read_dir(&artifacts)? {
        let project = project?;
        if !project.path().is_dir() {
            continue;
        }

        let mut entries = Vec::new();
        for entry in fs::read_dir(project.path())? {
            let entry = entry?;
            if entry.file_name() == "latest" {
                continue;
            }
            let metadata = entry.metadata()?;
            let modified: DateTime<Utc> = metadata.modified()?.into();
            entries.push((entry.path(), modified));
        }
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.1));

        for (idx, (path, modified)) in entries.into_iter().enumerate() {
            let beyond_keep = idx >= keep_artifacts;
            let beyond_age = cutoff.map(|cutoff| modified < cutoff).unwrap_or(false);
            if beyond_keep || beyond_age {
                remove_path(&path)?;
                println!("Pruned {}", path.display());
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::{Duration as StdDuration, SystemTime, UNIX_EPOCH};

    fn unique_temp_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{name}-{unique}"))
    }

    #[test]
    fn cache_prune_keeps_newest_artifacts() {
        let root = unique_temp_path("kokuban-prune-test");
        let project_dir = root.join("artifacts/test_project");
        fs::create_dir_all(project_dir.join("old")).unwrap();
        thread::sleep(StdDuration::from_millis(5));
        fs::create_dir_all(project_dir.join("new")).unwrap();

        handle_cache_prune(1, None, Some(root.clone())).unwrap();

        assert!(!project_dir.join("old").exists());
        assert!(project_dir.join("new").exists());
        fs::remove_dir_all(root).unwrap();
    }

    // A project name that escapes the cache root must never delete anything outside it.
    #[test]
    fn cache_clean_project_cannot_escape_root() {
        let root = unique_temp_path("kokuban-clean-escape-test");
        let victim = root.parent().unwrap().join(format!(
            "kokuban-clean-victim-{}",
            root.file_name().unwrap().to_string_lossy()
        ));
        fs::create_dir_all(&victim).unwrap();
        fs::write(victim.join("KEEP.txt"), "keep").unwrap();

        let escaped = format!("../../{}", victim.file_name().unwrap().to_string_lossy());
        let result = handle_cache_clean("project".to_string(), Some(escaped), Some(root.clone()));

        assert!(result.is_err(), "escaping project name must be rejected");
        assert!(
            victim.join("KEEP.txt").exists(),
            "files outside the cache root must be left untouched"
        );
        let _ = fs::remove_dir_all(&victim);
        let _ = fs::remove_dir_all(&root);
    }

    // Every shape that is not exactly one plain path segment must be refused, including
    // nested separators and the bare "." / ".." names that resolve to real directories.
    #[test]
    fn cache_clean_rejects_non_segment_project_names() {
        for name in [".", "..", "a/b", "/abs/path", "a/../../b", "./x"] {
            let root = unique_temp_path("kokuban-clean-shape-test");
            fs::create_dir_all(&root).unwrap();

            let result = handle_cache_clean(
                "project".to_string(),
                Some(name.to_string()),
                Some(root.clone()),
            );

            assert!(
                result.is_err(),
                "project name {:?} must be rejected, not cleaned",
                name
            );
            assert!(root.exists(), "root must survive rejected name {:?}", name);
            let _ = fs::remove_dir_all(&root);
        }
    }
}
