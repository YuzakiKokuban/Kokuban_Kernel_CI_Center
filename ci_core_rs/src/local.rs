use anyhow::{Context, Result, anyhow};
use chrono::{FixedOffset, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::config::ProjectConfig;
use crate::utils::{
    cache_file_name, command_exists, file_sha256, get_root_dir, load_project, run_cmd,
};

#[derive(Debug)]
struct LocalLock {
    path: PathBuf,
}

impl Drop for LocalLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Debug, Serialize)]
struct ToolchainCacheInfo {
    url: String,
    cache_path: String,
    configured_sha256: Option<String>,
    actual_sha256: Option<String>,
}

#[derive(Debug, Serialize)]
struct BuildInfo {
    project: String,
    repo: String,
    branch: String,
    variant: String,
    kernel_commit: Option<String>,
    apply_susfs: bool,
    apply_bbg: bool,
    apply_rekernel: bool,
    apply_zram: bool,
    local_root: String,
    workspace: String,
    kernel_source: String,
    artifact_dir: String,
    log_path: String,
    build_id: String,
    started_at_hkt: String,
    finished_at_hkt: String,
    host_os: String,
    host_arch: String,
    success: bool,
    failure_summary: Option<String>,
    toolchains: Vec<ToolchainCacheInfo>,
}

pub struct LocalBuildOptions {
    pub project: String,
    pub branch: String,
    pub variant: String,
    pub do_release: bool,
    pub custom_localversion: Option<String>,
    pub resukisu_setup_arg: Option<String>,
    pub apply_susfs: bool,
    pub apply_bbg: bool,
    pub apply_rekernel: bool,
    pub apply_zram: bool,
    pub local_root: Option<PathBuf>,
    pub offline: bool,
    pub no_fetch: bool,
    pub clean: bool,
    pub dry_run: bool,
    pub force_lock: bool,
}

pub fn ensure_local_host() -> Result<()> {
    if env::consts::OS != "linux" || env::consts::ARCH != "x86_64" {
        return Err(anyhow!(
            "Local mode currently supports x86-64 Linux only; detected {}-{}",
            env::consts::ARCH,
            env::consts::OS
        ));
    }

    let required = [
        "git", "make", "curl", "wget", "tar", "unzip", "zip", "patch", "nproc", "bc", "bison",
        "flex", "cpio", "lz4", "pahole",
    ];
    let missing: Vec<&str> = required
        .iter()
        .copied()
        .filter(|cmd| !command_exists(cmd))
        .collect();

    if !missing.is_empty() {
        return Err(anyhow!(
            "Missing local build dependencies: {}\nUbuntu install hint:\n  sudo apt-get update && sudo apt-get install -y build-essential git libncurses5-dev bc bison flex libssl-dev p7zip-full lz4 cpio curl wget libelf-dev dwarves jq lld pahole libdw-dev unzip zip",
            missing.join(", ")
        ));
    }

    Ok(())
}

pub fn default_local_root() -> Result<PathBuf> {
    if let Some(path) = env::var_os("KOKUBAN_LOCAL_ROOT").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }

    if let Some(path) = env::var_os("XDG_CACHE_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path).join("kokuban-kernel-ci"));
    }

    let home = env::var_os("HOME")
        .ok_or_else(|| anyhow!("Unable to locate HOME; pass --local-root explicitly"))?;
    Ok(PathBuf::from(home).join(".cache/kokuban-kernel-ci"))
}

fn lock_holder_pid(path: &Path) -> Option<u32> {
    let content = fs::read_to_string(path).ok()?;
    content.lines().find_map(|line| {
        line.strip_prefix("pid=")
            .and_then(|pid| pid.trim().parse::<u32>().ok())
    })
}

fn pid_is_alive(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

fn acquire_project_lock(local_root: &Path, project: &str, force: bool) -> Result<LocalLock> {
    let lock_dir = local_root.join("locks");
    fs::create_dir_all(&lock_dir)?;
    let path = lock_dir.join(format!("{}.lock", sanitize_path_component(project)));

    if force {
        let _ = fs::remove_file(&path);
    } else if path.exists() {
        if let Some(pid) = lock_holder_pid(&path) {
            if !pid_is_alive(pid) {
                eprintln!(
                    "Removing stale lock for project {} held by dead pid {}.",
                    project, pid
                );
                let _ = fs::remove_file(&path);
            }
        }
    }

    match fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
    {
        Ok(mut file) => {
            writeln!(file, "pid={}", std::process::id())?;
            writeln!(file, "created_at={}", Utc::now().to_rfc3339())?;
            Ok(LocalLock { path })
        }
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            let holder = fs::read_to_string(&path).unwrap_or_default();
            Err(anyhow!(
                "Another local build is already using project {}.\nLock: {}\n{}",
                project,
                path.display(),
                holder
            ))
        }
        Err(err) => Err(err.into()),
    }
}

pub fn sanitize_path_component(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            output.push(ch);
        } else {
            output.push('-');
        }
    }

    let trimmed = output.trim_matches('-');
    if trimmed.is_empty() {
        "default".to_string()
    } else {
        trimmed.to_string()
    }
}

fn repo_clone_url(repo: &str) -> String {
    if repo.contains("://") || repo.starts_with("git@") {
        repo.to_string()
    } else {
        format!("https://github.com/{}.git", repo.trim_end_matches(".git"))
    }
}

fn kernel_mirror_dir(local_root: &Path, project: &str) -> PathBuf {
    local_root
        .join("repos/kernels")
        .join(format!("{}.git", sanitize_path_component(project)))
}

fn git_rev_exists(repo_dir: &Path, rev: &str) -> bool {
    run_cmd(
        &["git", "rev-parse", "--verify", "--quiet", rev],
        Some(repo_dir),
        true,
    )
    .is_ok()
}

fn prepare_kernel_mirror(
    project: &str,
    proj: &ProjectConfig,
    mirror_dir: &Path,
    offline: bool,
    no_fetch: bool,
) -> Result<()> {
    let repo_url = repo_clone_url(&proj.repo);

    if !mirror_dir.join("HEAD").exists() {
        if offline {
            return Err(anyhow!(
                "Kernel mirror cache is missing for {}; disable --offline for the first run",
                project
            ));
        }

        fs::create_dir_all(
            mirror_dir
                .parent()
                .ok_or_else(|| anyhow!("Invalid kernel mirror path"))?,
        )?;
        run_cmd(
            &[
                "git",
                "clone",
                "--mirror",
                &repo_url,
                mirror_dir
                    .to_str()
                    .ok_or_else(|| anyhow!("Invalid kernel mirror path"))?,
            ],
            None,
            false,
        )
        .with_context(|| format!("Failed to clone kernel mirror from {repo_url}"))?;
    }

    run_cmd(
        &["git", "remote", "set-url", "origin", &repo_url],
        Some(mirror_dir),
        false,
    )?;

    if !offline && !no_fetch {
        run_cmd(
            &["git", "remote", "update", "--prune"],
            Some(mirror_dir),
            false,
        )
        .with_context(|| format!("Failed to update mirror for {project}"))?;
    }

    Ok(())
}

fn prepare_kernel_source(
    project: &str,
    proj: &ProjectConfig,
    branch: &str,
    mirror_dir: &Path,
    source_dir: &Path,
    offline: bool,
    no_fetch: bool,
    clean: bool,
) -> Result<()> {
    let repo_url = repo_clone_url(&proj.repo);
    prepare_kernel_mirror(project, proj, mirror_dir, offline, no_fetch)?;

    if !source_dir.join(".git").exists() {
        fs::create_dir_all(
            source_dir
                .parent()
                .ok_or_else(|| anyhow!("Invalid kernel source path"))?,
        )?;
        let source_path = source_dir
            .to_str()
            .ok_or_else(|| anyhow!("Invalid kernel source path"))?;
        let mirror_path = mirror_dir
            .to_str()
            .ok_or_else(|| anyhow!("Invalid kernel mirror path"))?;

        if offline || no_fetch {
            run_cmd(
                &["git", "clone", "--recursive", mirror_path, source_path],
                None,
                false,
            )
            .with_context(|| format!("Failed to clone kernel source from mirror {mirror_path}"))?;
        } else {
            run_cmd(
                &[
                    "git",
                    "clone",
                    "--recursive",
                    "--reference-if-able",
                    mirror_path,
                    &repo_url,
                    source_path,
                ],
                None,
                false,
            )
            .with_context(|| format!("Failed to clone kernel source from {repo_url}"))?;
        }
    }

    run_cmd(
        &["git", "remote", "set-url", "origin", &repo_url],
        Some(source_dir),
        false,
    )?;

    if !offline && !no_fetch {
        run_cmd(
            &["git", "fetch", "--tags", "--prune", "origin"],
            Some(source_dir),
            false,
        )
        .with_context(|| format!("Failed to fetch updates for {project}"))?;
    }

    let remote_ref = format!("refs/remotes/origin/{branch}");
    let checkout_target = if git_rev_exists(source_dir, &remote_ref) {
        format!("origin/{branch}")
    } else {
        branch.to_string()
    };

    run_cmd(
        &["git", "checkout", "--force", &checkout_target],
        Some(source_dir),
        false,
    )
    .with_context(|| format!("Failed to checkout {branch} for {project}"))?;
    run_cmd(&["git", "reset", "--hard", "HEAD"], Some(source_dir), false)?;

    if clean {
        run_cmd(&["git", "clean", "-ffdx"], Some(source_dir), false)?;
    } else {
        run_cmd(
            &["git", "clean", "-fd", "-e", "out", "-e", ".ccache"],
            Some(source_dir),
            false,
        )?;
    }

    if source_dir.join(".gitmodules").exists() {
        if !offline && !no_fetch {
            run_cmd(
                &["git", "submodule", "sync", "--recursive"],
                Some(source_dir),
                false,
            )?;
        }
        run_cmd(
            &["git", "submodule", "update", "--init", "--recursive"],
            Some(source_dir),
            false,
        )?;
    }

    Ok(())
}

fn resolved_variant(branch: &str, variant: &str) -> String {
    let trimmed = variant.trim();
    if trimmed.is_empty() || trimmed == "default" {
        branch.to_string()
    } else {
        trimmed.to_string()
    }
}

fn add_optional_arg(command: &mut Command, name: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        command.arg(format!("{name}={value}"));
    }
}

fn run_build_command(
    options: &LocalBuildOptions,
    central_root: &Path,
    run_dir: &Path,
    local_root: &Path,
    variant: &str,
    log_path: &Path,
) -> Result<()> {
    let mut command = Command::new(env::current_exe()?);
    command
        .arg("build")
        .arg("--project")
        .arg(&options.project)
        .arg("--branch")
        .arg(variant)
        .arg("--do-release")
        .arg(options.do_release.to_string())
        .arg("--apply-susfs")
        .arg(options.apply_susfs.to_string())
        .arg("--apply-bbg")
        .arg(options.apply_bbg.to_string())
        .arg("--apply-rekernel")
        .arg(options.apply_rekernel.to_string())
        .arg("--apply-zram")
        .arg(options.apply_zram.to_string())
        .current_dir(run_dir)
        .env("CI_CENTRAL_ROOT", central_root)
        .env("KOKUBAN_REUSE_TOOLCHAINS", "1")
        .env(
            "KOKUBAN_DOWNLOAD_CACHE_DIR",
            local_root.join("downloads/toolchains"),
        )
        .env(
            "KOKUBAN_ANYKERNEL_CACHE_DIR",
            local_root.join("repos/AnyKernel3"),
        )
        .env("SCCACHE_DIR", local_root.join("sccache"))
        .env("CCACHE_DIR", local_root.join("ccache"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if options.offline {
        command.env("KOKUBAN_OFFLINE", "1");
    }

    add_optional_arg(
        &mut command,
        "--custom-localversion",
        options.custom_localversion.as_deref(),
    );
    add_optional_arg(
        &mut command,
        "--resukisu-setup-arg",
        options.resukisu_setup_arg.as_deref(),
    );

    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let log_file = Arc::new(Mutex::new(File::create(log_path)?));
    writeln!(log_file.lock().unwrap(), "Command: {:?}", command)?;

    let mut child = command.spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("Failed to capture build stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("Failed to capture build stderr"))?;

    let stdout_log = Arc::clone(&log_file);
    let stdout_handle = thread::spawn(move || tee_stream(stdout, std::io::stdout(), stdout_log));
    let stderr_log = Arc::clone(&log_file);
    let stderr_handle = thread::spawn(move || tee_stream(stderr, std::io::stderr(), stderr_log));

    let status = child.wait()?;
    stdout_handle
        .join()
        .map_err(|_| anyhow!("Failed to join build stdout logger"))??;
    stderr_handle
        .join()
        .map_err(|_| anyhow!("Failed to join build stderr logger"))??;

    if !status.success() {
        let summary = failure_summary(log_path)?;
        if !summary.is_empty() {
            eprintln!("\nFailure summary:\n{}", summary);
        }
        return Err(anyhow!("Local build failed with status {}", status));
    }

    Ok(())
}

fn failure_summary(log_path: &Path) -> Result<String> {
    if !log_path.is_file() {
        return Ok(String::new());
    }

    let content = fs::read_to_string(log_path).unwrap_or_default();
    let keywords = [
        "error:",
        "fatal:",
        "undefined reference",
        "No such file",
        "not found",
        "failed",
        "FAILED",
        "Error ",
    ];
    let mut hits = Vec::new();
    for line in content.lines() {
        if keywords.iter().any(|keyword| line.contains(keyword)) {
            hits.push(line.to_string());
        }
    }

    let mut lines = Vec::new();
    if !hits.is_empty() {
        lines.push("Matched error lines:".to_string());
        for line in hits.iter().rev().take(40).rev() {
            lines.push(format!("  {}", line));
        }
    }

    lines.push("Last log lines:".to_string());
    let tail: Vec<&str> = content.lines().rev().take(120).collect();
    for line in tail.into_iter().rev() {
        lines.push(format!("  {}", line));
    }

    Ok(lines.join("\n"))
}

fn tee_stream<R, W>(mut reader: R, mut writer: W, log_file: Arc<Mutex<File>>) -> Result<()>
where
    R: Read,
    W: Write,
{
    let mut buffer = [0_u8; 8192];
    loop {
        let len = reader.read(&mut buffer)?;
        if len == 0 {
            break;
        }
        writer.write_all(&buffer[..len])?;
        writer.flush()?;
        let mut log = log_file.lock().unwrap();
        log.write_all(&buffer[..len])?;
        log.flush()?;
    }
    Ok(())
}

fn copy_if_exists(source: &Path, dest_dir: &Path) -> Result<bool> {
    if !source.is_file() {
        return Ok(false);
    }
    let file_name = source
        .file_name()
        .ok_or_else(|| anyhow!("Invalid artifact path: {}", source.display()))?;
    fs::copy(source, dest_dir.join(file_name))?;
    Ok(true)
}

fn replace_latest_symlink(latest_path: &Path, target: &Path) -> Result<()> {
    if fs::symlink_metadata(latest_path).is_ok() {
        if latest_path.is_dir() && !latest_path.is_symlink() {
            fs::remove_dir_all(latest_path)?;
        } else {
            fs::remove_file(latest_path)?;
        }
    }
    symlink(target, latest_path)?;
    Ok(())
}

fn archive_artifacts(
    local_root: &Path,
    project: &str,
    run_dir: &Path,
    build_id: &str,
    log_path: &Path,
) -> Result<PathBuf> {
    let artifact_dir = local_root.join("artifacts").join(project).join(build_id);
    fs::create_dir_all(&artifact_dir)?;

    let mut copied = false;
    for entry in fs::read_dir(run_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("zip") {
            copied |= copy_if_exists(&path, &artifact_dir)?;
        }
    }

    for source in [
        run_dir.join("kernel_source/out/.config"),
        run_dir.join("kernel_source/out/vmlinux.symvers"),
        log_path.to_path_buf(),
    ] {
        copied |= copy_if_exists(&source, &artifact_dir)?;
    }

    let latest_path = local_root.join("artifacts").join(project).join("latest");
    replace_latest_symlink(&latest_path, &artifact_dir)?;

    if copied {
        println!("Archived local artifacts into {}", artifact_dir.display());
    } else {
        println!(
            "Created artifact directory {}, but no build artifacts were found.",
            artifact_dir.display()
        );
    }

    Ok(artifact_dir)
}

fn collect_toolchain_info(
    local_root: &Path,
    proj: &ProjectConfig,
) -> Result<Vec<ToolchainCacheInfo>> {
    let mut items = Vec::new();
    let Some(urls) = &proj.toolchain_urls else {
        return Ok(items);
    };

    let checksums: Option<&HashMap<String, String>> = proj.toolchain_sha256.as_ref();
    for url in urls {
        let cache_path = local_root
            .join("downloads/toolchains")
            .join(cache_file_name(url)?);
        let actual_sha256 = if cache_path.is_file() {
            Some(file_sha256(&cache_path)?)
        } else {
            None
        };

        items.push(ToolchainCacheInfo {
            url: url.clone(),
            cache_path: cache_path.display().to_string(),
            configured_sha256: checksums.and_then(|map| map.get(url)).cloned(),
            actual_sha256,
        });
    }

    Ok(items)
}

fn write_build_info(path: &Path, info: &BuildInfo) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(info)? + "\n")?;
    Ok(())
}

fn print_local_outputs(run_dir: &Path) -> Result<()> {
    let mut zipballs = Vec::new();
    for entry in fs::read_dir(run_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("zip") {
            zipballs.push(path);
        }
    }
    zipballs.sort();

    println!("Local build finished.");
    println!("Workspace: {}", run_dir.display());
    println!("Kernel source: {}", run_dir.join("kernel_source").display());
    if !zipballs.is_empty() {
        println!("Artifacts:");
        for zipball in zipballs {
            println!("  {}", zipball.display());
        }
    }

    Ok(())
}

fn local_layout(
    options: &LocalBuildOptions,
    local_root: &Path,
) -> (String, String, PathBuf, PathBuf, PathBuf) {
    let project_key = sanitize_path_component(&options.project);
    let branch_key = sanitize_path_component(&options.branch);
    let variant = resolved_variant(&options.branch, &options.variant);
    let variant_key = sanitize_path_component(&variant);
    let run_dir = local_root
        .join("builds")
        .join(project_key)
        .join(format!("{branch_key}-{variant_key}"));
    let source_dir = run_dir.join("kernel_source");
    let mirror_dir = kernel_mirror_dir(local_root, &options.project);
    (variant, variant_key, run_dir, source_dir, mirror_dir)
}

fn print_plan(
    options: &LocalBuildOptions,
    central_root: &Path,
    local_root: &Path,
    variant: &str,
    run_dir: &Path,
    source_dir: &Path,
    mirror_dir: &Path,
    build_id: &str,
) {
    let project_key = sanitize_path_component(&options.project);
    println!("Local build plan");
    println!("  project: {}", options.project);
    println!("  branch: {}", options.branch);
    println!("  variant: {}", variant);
    println!("  release: {}", options.do_release);
    println!("  apply_susfs: {}", options.apply_susfs);
    println!("  apply_bbg: {}", options.apply_bbg);
    println!("  apply_rekernel: {}", options.apply_rekernel);
    println!("  apply_zram: {}", options.apply_zram);
    println!("  offline: {}", options.offline);
    println!("  no_fetch: {}", options.no_fetch);
    println!("  clean: {}", options.clean);
    println!("  force_lock: {}", options.force_lock);
    println!("  central_root: {}", central_root.display());
    println!("  local_root: {}", local_root.display());
    println!("  mirror: {}", mirror_dir.display());
    println!("  workspace: {}", run_dir.display());
    println!("  kernel_source: {}", source_dir.display());
    println!(
        "  toolchain_cache: {}",
        local_root.join("downloads/toolchains").display()
    );
    println!(
        "  anykernel_cache: {}",
        local_root.join("repos/AnyKernel3").display()
    );
    println!(
        "  log: {}",
        local_root
            .join("logs")
            .join(&project_key)
            .join(format!("{build_id}.log"))
            .display()
    );
    println!(
        "  artifact_dir: {}",
        local_root
            .join("artifacts")
            .join(&project_key)
            .join(build_id)
            .display()
    );
}

pub fn handle_local_build(options: LocalBuildOptions) -> Result<()> {
    let central_root = fs::canonicalize(get_root_dir()).context("Failed to locate CI root")?;
    let proj = load_project(&options.project)?;
    let local_root = options.local_root.clone().unwrap_or(default_local_root()?);
    let project_key = sanitize_path_component(&options.project);
    let (variant, _variant_key, run_dir, source_dir, mirror_dir) =
        local_layout(&options, &local_root);
    let hkt = FixedOffset::east_opt(8 * 3600).ok_or_else(|| anyhow!("Invalid HKT offset"))?;
    let build_id = Utc::now()
        .with_timezone(&hkt)
        .format("%Y%m%d-%H%M%S")
        .to_string();
    let started_at_hkt = Utc::now().with_timezone(&hkt).to_rfc3339();
    let log_path = local_root
        .join("logs")
        .join(&project_key)
        .join(format!("{build_id}.log"));

    print_plan(
        &options,
        &central_root,
        &local_root,
        &variant,
        &run_dir,
        &source_dir,
        &mirror_dir,
        &build_id,
    );

    if options.dry_run {
        return Ok(());
    }

    ensure_local_host()?;
    let _lock = acquire_project_lock(&local_root, &options.project, options.force_lock)?;

    fs::create_dir_all(&run_dir)?;
    fs::create_dir_all(local_root.join("downloads/toolchains"))?;
    fs::create_dir_all(local_root.join("repos"))?;
    fs::create_dir_all(local_root.join("repos/kernels"))?;
    fs::create_dir_all(local_root.join("artifacts"))?;
    fs::create_dir_all(local_root.join("logs"))?;
    fs::create_dir_all(local_root.join("sccache"))?;
    fs::create_dir_all(local_root.join("ccache"))?;

    println!("Local root: {}", local_root.display());
    println!(
        "Preparing kernel source for {} @ {}",
        options.project, options.branch
    );
    prepare_kernel_source(
        &options.project,
        &proj,
        &options.branch,
        &mirror_dir,
        &source_dir,
        options.offline,
        options.no_fetch,
        options.clean,
    )?;

    let kernel_commit = run_cmd(&["git", "rev-parse", "HEAD"], Some(&source_dir), true)
        .ok()
        .flatten();

    println!("Starting local build with variant: {variant}");
    let build_result = run_build_command(
        &options,
        &central_root,
        &run_dir,
        &local_root,
        &variant,
        &log_path,
    );
    let artifact_dir =
        archive_artifacts(&local_root, &project_key, &run_dir, &build_id, &log_path)?;
    let failure_summary = if build_result.is_err() {
        Some(failure_summary(&log_path).unwrap_or_default())
    } else {
        None
    };
    let finished_at_hkt = Utc::now().with_timezone(&hkt).to_rfc3339();
    let build_info = BuildInfo {
        project: options.project.clone(),
        repo: proj.repo.clone(),
        branch: options.branch.clone(),
        variant: variant.clone(),
        kernel_commit,
        apply_susfs: options.apply_susfs,
        apply_bbg: options.apply_bbg,
        apply_rekernel: options.apply_rekernel,
        apply_zram: options.apply_zram,
        local_root: local_root.display().to_string(),
        workspace: run_dir.display().to_string(),
        kernel_source: source_dir.display().to_string(),
        artifact_dir: artifact_dir.display().to_string(),
        log_path: log_path.display().to_string(),
        build_id: build_id.clone(),
        started_at_hkt,
        finished_at_hkt,
        host_os: env::consts::OS.to_string(),
        host_arch: env::consts::ARCH.to_string(),
        success: build_result.is_ok(),
        failure_summary,
        toolchains: collect_toolchain_info(&local_root, &proj)?,
    };
    write_build_info(&artifact_dir.join("build-info.json"), &build_info)?;
    build_result?;
    print_local_outputs(&run_dir)?;
    println!("Build log: {}", log_path.display());
    println!("Artifact archive: {}", artifact_dir.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("{name}-{unique}"))
    }

    #[test]
    fn sanitizes_path_components() {
        assert_eq!(
            sanitize_path_component("s25/sm8750 resukisu"),
            "s25-sm8750-resukisu"
        );
        assert_eq!(sanitize_path_component("///"), "default");
    }

    #[test]
    fn failure_summary_extracts_error_lines_and_tail() {
        let path = unique_temp_path("kokuban-log-test");
        fs::write(&path, "line 1\nwarning\nfatal: missing file\nlast line\n").unwrap();
        let summary = failure_summary(&path).unwrap();
        assert!(summary.contains("Matched error lines:"));
        assert!(summary.contains("fatal: missing file"));
        assert!(summary.contains("Last log lines:"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn force_lock_replaces_existing_lock() {
        let root = unique_temp_path("kokuban-lock-test");
        let lock_dir = root.join("locks");
        fs::create_dir_all(&lock_dir).unwrap();
        fs::write(lock_dir.join("test.lock"), "pid=1\n").unwrap();

        let lock = acquire_project_lock(&root, "test", true).unwrap();
        assert!(lock.path.exists());
        drop(lock);
        assert!(!lock_dir.join("test.lock").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
