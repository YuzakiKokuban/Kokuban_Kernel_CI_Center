use anyhow::{Result, anyhow};
use chrono::Local;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::config::{BbgConfig, ProjectConfig, SusfsConfig};
use crate::utils::{handle_notify, load_projects, run_cmd, run_cmd_with_env, set_github_output};

fn command_exists(command: &str) -> bool {
    std::process::Command::new(command)
        .arg("-V")
        .output()
        .is_ok()
}

fn create_compiler_wrapper(
    wrapper_dir: &Path,
    wrapper_name: &str,
    command_prefix: &str,
    tool: &str,
) -> Result<String> {
    let wrapper_path = wrapper_dir.join(wrapper_name);
    fs::write(
        &wrapper_path,
        format!("#!/bin/sh\nexec {} {} \"$@\"\n", command_prefix, tool),
    )?;
    fs::set_permissions(&wrapper_path, PermissionsExt::from_mode(0o755))?;
    Ok(wrapper_path.to_string_lossy().to_string())
}

fn copy_artifact_if_exists(source: &Path, artifact_dir: &Path) -> Result<bool> {
    if !source.is_file() {
        return Ok(false);
    }

    let file_name = source
        .file_name()
        .ok_or_else(|| anyhow!("Artifact path {:?} has no filename", source))?;
    fs::copy(source, artifact_dir.join(file_name))?;
    Ok(true)
}

fn upsert_kconfig_entry(content: &str, key: &str, value: &str) -> String {
    let key_prefix = format!("{key}=");
    let not_set_line = format!("# {key} is not set");
    let replacement = format!("{key}={value}");

    let mut found = false;
    let mut lines = Vec::new();

    for line in content.lines() {
        if line.starts_with(&key_prefix) || line == not_set_line {
            if !found {
                lines.push(replacement.clone());
                found = true;
            }
            continue;
        }

        lines.push(line.to_string());
    }

    if !found {
        lines.push(replacement);
    }

    lines.join("\n") + "\n"
}

fn truncate_to_len(input: &str, max_len: usize) -> String {
    input.chars().take(max_len).collect()
}

fn build_sm8750_localversion(base: &str, short_sha: &str, kernel_version: &str) -> Result<String> {
    const UNAME_MAX_VISIBLE_LEN: usize = 63;

    let normalized_base = if base.trim().is_empty() {
        "-Kokuban".to_string()
    } else {
        format!("-{}", base.trim().trim_start_matches('-'))
    };
    let commit_suffix = format!("-g{}", short_sha);

    if kernel_version.len() >= UNAME_MAX_VISIBLE_LEN {
        return Err(anyhow!(
            "kernelversion is too long for sm8750 uname limit: {}",
            kernel_version
        ));
    }

    let max_localversion_len = UNAME_MAX_VISIBLE_LEN.saturating_sub(kernel_version.len());
    if commit_suffix.len() > max_localversion_len {
        return Err(anyhow!(
            "Not enough uname budget for sm8750 localversion suffix {}",
            commit_suffix
        ));
    }

    let max_base_len = max_localversion_len - commit_suffix.len();
    let truncated_base = truncate_to_len(&normalized_base, max_base_len);

    Ok(format!("{}{}", truncated_base, commit_suffix))
}

fn find_first_existing_path(base: &Path, candidates: &[String]) -> Option<PathBuf> {
    candidates
        .iter()
        .map(|candidate| base.join(candidate))
        .find(|path| path.exists())
}

fn find_first_existing_dir(base: &Path, candidates: &[String]) -> Option<PathBuf> {
    candidates
        .iter()
        .map(|candidate| base.join(candidate))
        .find(|path| path.is_dir())
}

fn copy_dir_files(source: &Path, dest: &Path) -> Result<()> {
    if !source.is_dir() {
        return Err(anyhow!("Source directory not found: {:?}", source));
    }

    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let file_name = path
                .file_name()
                .ok_or_else(|| anyhow!("Invalid file path: {:?}", path))?;
            fs::copy(&path, dest.join(file_name))?;
        }
    }

    Ok(())
}

fn can_apply_patch(patch_file: &Path, cwd: &Path) -> Result<bool> {
    let status = std::process::Command::new("patch")
        .arg("-p1")
        .arg("-N")
        .arg("-F")
        .arg("3")
        .arg("--dry-run")
        .arg("-i")
        .arg(patch_file)
        .current_dir(cwd)
        .status()?;
    Ok(status.success())
}

fn run_patch(patch_file: &Path, cwd: &Path) -> Result<bool> {
    let status = std::process::Command::new("patch")
        .arg("-p1")
        .arg("-N")
        .arg("-F")
        .arg("3")
        .arg("-i")
        .arg(patch_file)
        .current_dir(cwd)
        .status()?;
    Ok(status.success())
}

fn apply_patch_with_fallbacks(
    patch_file: &Path,
    kernel_source_path: &Path,
    fallback_dirs: &[String],
) -> Result<()> {
    if can_apply_patch(patch_file, kernel_source_path)?
        && run_patch(patch_file, kernel_source_path)?
    {
        return Ok(());
    }

    for fallback in fallback_dirs {
        let cwd = kernel_source_path.join(fallback);
        if cwd.is_dir() && can_apply_patch(patch_file, &cwd)? && run_patch(patch_file, &cwd)? {
            return Ok(());
        }
    }

    Err(anyhow!("Failed to apply patch {:?}", patch_file))
}

fn ensure_bbg_lsm(content: &str) -> String {
    let mut lines = Vec::new();
    let mut in_lsm_block = false;

    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("config LSM") {
            in_lsm_block = true;
        } else if in_lsm_block
            && !trimmed.is_empty()
            && !line.starts_with(' ')
            && !line.starts_with('\t')
            && !trimmed.starts_with("help")
        {
            in_lsm_block = false;
        }

        if in_lsm_block && trimmed.starts_with("default") && line.contains("selinux") {
            if line.contains("baseband_guard") {
                lines.push(line.to_string());
            } else {
                lines.push(line.replacen("selinux", "selinux,baseband_guard", 1));
            }
            continue;
        }

        lines.push(line.to_string());
    }

    lines.join("\n") + "\n"
}

fn apply_susfs_overlay(
    kernel_source_path: &Path,
    susfs: &SusfsConfig,
    branch: &str,
) -> Result<bool> {
    let temp_dir = kernel_source_path.join(".susfs_workspace");
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir)?;
    }

    run_cmd(
        &[
            "git",
            "clone",
            "--depth=1",
            "--branch",
            &susfs.branch,
            &susfs.repo,
            temp_dir
                .to_str()
                .ok_or_else(|| anyhow!("Invalid temp path"))?,
        ],
        None,
        false,
    )?;

    let patch_path = temp_dir.join(&susfs.patch_path);
    if !patch_path.exists() {
        fs::remove_dir_all(&temp_dir)?;
        return Err(anyhow!("SuSFS patch not found: {:?}", patch_path));
    }

    let fs_source = temp_dir.join(susfs.fs_patch_dir.as_deref().unwrap_or("kernel_patches/fs"));
    let fs_target = find_first_existing_dir(
        kernel_source_path,
        &[
            "common/fs".to_string(),
            "kernel_platform/common/fs".to_string(),
            "fs".to_string(),
        ],
    )
    .ok_or_else(|| anyhow!("Could not locate kernel fs directory for SuSFS"))?;
    copy_dir_files(&fs_source, &fs_target)?;

    let include_source = temp_dir.join(
        susfs
            .include_linux_patch_dir
            .as_deref()
            .unwrap_or("kernel_patches/include/linux"),
    );
    let include_target = find_first_existing_dir(
        kernel_source_path,
        &[
            "common/include/linux".to_string(),
            "kernel_platform/common/include/linux".to_string(),
            "include/linux".to_string(),
        ],
    )
    .ok_or_else(|| anyhow!("Could not locate kernel include/linux directory for SuSFS"))?;
    copy_dir_files(&include_source, &include_target)?;

    apply_patch_with_fallbacks(
        &patch_path,
        kernel_source_path,
        &["common".to_string(), "kernel_platform/common".to_string()],
    )?;

    let mut ksu_patch_applied = false;
    if let Some(ksu_patch_path) = &susfs.ksu_patch_path {
        let ksu_patch = temp_dir.join(ksu_patch_path);
        if ksu_patch.exists() {
            if let Some(ksu_dir) = find_first_existing_dir(
                kernel_source_path,
                &[
                    "KernelSU".to_string(),
                    "KernelSU-Next".to_string(),
                    "ReSukiSU".to_string(),
                ],
            ) {
                if branch == "ksu" || branch == "mksu" || ksu_dir.ends_with("KernelSU") {
                    if can_apply_patch(&ksu_patch, &ksu_dir)? && run_patch(&ksu_patch, &ksu_dir)? {
                        ksu_patch_applied = true;
                    }
                }
            }
        }
    }

    fs::remove_dir_all(&temp_dir)?;
    Ok(ksu_patch_applied)
}

fn apply_bbg_overlay(
    kernel_source_path: &Path,
    proj: &ProjectConfig,
    bbg: Option<&BbgConfig>,
) -> Result<()> {
    let defconfig_path = find_first_existing_path(
        kernel_source_path,
        &[
            format!("arch/arm64/configs/{}", proj.defconfig),
            format!("common/arch/arm64/configs/{}", proj.defconfig),
            format!("kernel_platform/arch/arm64/configs/{}", proj.defconfig),
            format!(
                "kernel_platform/common/arch/arm64/configs/{}",
                proj.defconfig
            ),
        ],
    )
    .ok_or_else(|| anyhow!("Could not locate defconfig for BBG"))?;
    let defconfig_content = fs::read_to_string(&defconfig_path).unwrap_or_default();
    fs::write(
        &defconfig_path,
        upsert_kconfig_entry(&defconfig_content, "CONFIG_BBG", "y"),
    )?;

    let common_root = find_first_existing_dir(
        kernel_source_path,
        &[
            "common".to_string(),
            "kernel_platform/common".to_string(),
            ".".to_string(),
        ],
    )
    .ok_or_else(|| anyhow!("Could not locate common source root for BBG"))?;

    let setup_url = bbg
        .and_then(|cfg| cfg.setup_url.as_deref())
        .unwrap_or("https://github.com/cctv18/Baseband-guard/raw/master/setup.sh");
    let cmd = format!("curl -LSs '{}' | bash", setup_url);
    run_cmd(&["bash", "-c", &cmd], Some(&common_root), false)?;

    let security_kconfig = find_first_existing_path(
        &common_root,
        &[
            "security/Kconfig".to_string(),
            "../security/Kconfig".to_string(),
        ],
    )
    .or_else(|| {
        find_first_existing_path(
            kernel_source_path,
            &[
                "common/security/Kconfig".to_string(),
                "kernel_platform/common/security/Kconfig".to_string(),
                "security/Kconfig".to_string(),
            ],
        )
    })
    .ok_or_else(|| anyhow!("Could not locate security/Kconfig for BBG"))?;

    let security_content = fs::read_to_string(&security_kconfig).unwrap_or_default();
    fs::write(&security_kconfig, ensure_bbg_lsm(&security_content))?;

    Ok(())
}

fn apply_sm8850_localversion(
    kernel_source_path: &Path,
    defconfig_name: &str,
    localversion: &str,
) -> Result<()> {
    let setlocalversion_path = kernel_source_path.join("scripts/setlocalversion");
    if setlocalversion_path.exists() {
        let mut content = fs::read_to_string(&setlocalversion_path).unwrap_or_default();
        content = content.replace(" -dirty", "");

        let dirty_cleanup_line = r#"res=$(echo "$res" | sed 's/-dirty//g')"#;
        let final_release_echo = r#"echo "${KERNELVERSION}${file_localversion}${config_localversion}${LOCALVERSION}${scm_version}""#;

        if !content.contains(dirty_cleanup_line) {
            if let Some(final_echo_pos) = content.rfind(final_release_echo) {
                content.insert_str(final_echo_pos, &format!("{dirty_cleanup_line}\n"));
            } else {
                if !content.ends_with('\n') {
                    content.push('\n');
                }
                content.push_str(dirty_cleanup_line);
                content.push('\n');
            }
        }

        content = content.replace("${scm_version}", "");
        fs::write(&setlocalversion_path, content)?;
    }

    let defconfig_path = kernel_source_path.join(format!("arch/arm64/configs/{}", defconfig_name));
    if defconfig_path.exists() {
        let mut defconfig_content = fs::read_to_string(&defconfig_path).unwrap_or_default();
        defconfig_content = upsert_kconfig_entry(
            &defconfig_content,
            "CONFIG_LOCALVERSION",
            &format!("\"{}\"", localversion),
        );
        defconfig_content =
            upsert_kconfig_entry(&defconfig_content, "CONFIG_LOCALVERSION_AUTO", "n");
        fs::write(defconfig_path, defconfig_content)?;
    }

    Ok(())
}

pub fn handle_build(
    project_key: String,
    branch: String,
    do_release: bool,
    custom_localversion: Option<String>,
    custom_build_time: Option<String>,
    resukisu_setup_arg: Option<String>,
    apply_susfs: bool,
    apply_bbg: bool,
) -> Result<()> {
    let projects = load_projects()?;
    let proj_val = projects
        .get(&project_key)
        .ok_or_else(|| anyhow!("Project not found"))?;
    let proj: ProjectConfig = serde_json::from_value(proj_val.clone())?;

    let kernel_source_path = PathBuf::from("kernel_source");
    if !kernel_source_path.exists() {
        return Err(anyhow!("Kernel source not found at ./kernel_source"));
    }

    let target_soc_str = project_key.split('_').nth(1).unwrap_or("unknown");

    let wrapper_dir = env::current_dir()?.join(".compiler_wrappers");
    let _ = fs::create_dir_all(&wrapper_dir);

    let rust_cmd = if command_exists("sccache") {
        create_compiler_wrapper(&wrapper_dir, "rustc", "sccache", "rustc")?
    } else {
        "rustc".to_string()
    };

    let cc_cmd = if command_exists("sccache") {
        create_compiler_wrapper(&wrapper_dir, "clang", "sccache", "clang")?
    } else if command_exists("ccache") {
        create_compiler_wrapper(&wrapper_dir, "clang", "ccache", "clang")?
    } else {
        "clang".to_string()
    };

    let rustc_arg = format!("RUSTC={}", rust_cmd);
    let hostrustc_arg = format!("HOSTRUSTC={}", rust_cmd);
    let cc_arg = format!("CC={}", cc_cmd);
    let hostcc_arg = format!("HOSTCC={}", cc_cmd);

    if let Some(urls) = &proj.toolchain_urls {
        let tc_download_dir = PathBuf::from("toolchain_download");

        if tc_download_dir.exists() {
            fs::remove_dir_all(&tc_download_dir)?;
        }
        fs::create_dir_all(&tc_download_dir)?;

        for url in urls {
            println!("Downloading toolchain from {}...", url);
            run_cmd(&["wget", "-q", url], Some(&tc_download_dir), false)?;
        }

        let extract_script = r#"
            set -e
            if ls *.tar.gz.[0-9]* 1> /dev/null 2>&1; then
                cat *.tar.gz.* | tar -zxf - --warning=no-unknown-keyword -C ..
            elif ls *part_aa* 1> /dev/null 2>&1 || ls *_aa.tar.gz 1> /dev/null 2>&1 || ls *.tar.gz.aa 1> /dev/null 2>&1; then
                cat *.tar.gz | tar -zxf - --warning=no-unknown-keyword -C ..
            else
                if ls *.tar.gz 1> /dev/null 2>&1; then
                    for tarball in *.tar.gz; do
                        tar -zxf "$tarball" --warning=no-unknown-keyword -C ..
                    done
                fi
                if ls *.tar.xz 1> /dev/null 2>&1; then
                    for tarball in *.tar.xz; do
                        tar -xf "$tarball" -C ..
                    done
                fi
                if ls *.zip 1> /dev/null 2>&1; then
                    for zipball in *.zip; do
                        unzip -o -q "$zipball" -d ..
                    done
                fi
            fi
            chmod -R +x ../bin/ 2>/dev/null || true
            chmod -R +x ../build-tools/bin/ 2>/dev/null || true
            chmod +x ../bindgen-cli-*/bindgen 2>/dev/null || true
        "#;

        run_cmd(
            &["bash", "-c", extract_script],
            Some(&tc_download_dir),
            false,
        )?;

        fs::remove_dir_all(tc_download_dir)?;
    }

    let toolchain_prefix = proj.toolchain_path_prefix.as_deref().unwrap_or("");
    let toolchain_base = env::current_dir()?.join(toolchain_prefix);

    let mut build_env = HashMap::new();
    let current_path = env::var("PATH").unwrap_or_default();

    let mut new_path = current_path.clone();

    if let Some(exports) = &proj.toolchain_path_exports {
        for export in exports {
            let p = toolchain_base.join(export);
            new_path = format!("{}:{}", p.display(), new_path);
        }
    } else if !toolchain_prefix.is_empty() {
        new_path = format!("{}:{}", toolchain_base.join("bin").display(), new_path);
    }

    build_env.insert("PATH".to_string(), new_path);
    build_env.insert("ARCH".to_string(), "arm64".to_string());
    build_env.insert("SUBARCH".to_string(), "arm64".to_string());
    build_env.insert("CLANG_TRIPLE".to_string(), "aarch64-linux-gnu-".to_string());
    build_env.insert(
        "CROSS_COMPILE".to_string(),
        "aarch64-linux-gnu-".to_string(),
    );
    build_env.insert(
        "CROSS_COMPILE_COMPAT".to_string(),
        "arm-linux-gnueabi-".to_string(),
    );

    let mut kcflags = "-O2 -pipe -Wno-error -D__ANDROID_COMMON_KERNEL__".to_string();
    if target_soc_str == "sm8850" {
        if let Ok(common_real_path) = fs::canonicalize(&kernel_source_path) {
            if let Some(root_real_path) = common_real_path.parent() {
                kcflags = format!(
                    "-O2 -pipe -Wno-error -fno-stack-protector -no-canonical-prefixes -D__ANDROID_COMMON_KERNEL__ -fdebug-prefix-map={}=. -fmacro-prefix-map={}=. -ffile-prefix-map={}=.",
                    root_real_path.display(),
                    root_real_path.display(),
                    root_real_path.display()
                );
            }
        }
        let libclang_path = toolchain_base.join("lib");
        build_env.insert(
            "LIBCLANG_PATH".to_string(),
            libclang_path.display().to_string(),
        );
        build_env.insert("KBUILD_GENDWARFKSYMS_STABLE".to_string(), "1".to_string());
        build_env.insert("KBUILD_BUILD_USER".to_string(), "build-user".to_string());
        build_env.insert("KBUILD_BUILD_HOST".to_string(), "build-host".to_string());
        build_env.insert("TZ".to_string(), "UTC".to_string());
        build_env.insert("LC_ALL".to_string(), "C".to_string());
    }

    build_env.insert("RUSTC".to_string(), rust_cmd.clone());
    build_env.insert("HOSTRUSTC".to_string(), rust_cmd.clone());
    build_env.insert("BINDGEN".to_string(), "bindgen".to_string());

    build_env.insert("KCFLAGS".to_string(), kcflags.clone());
    build_env.insert("KCPPFLAGS".to_string(), kcflags);
    build_env.insert("IN_KERNEL_MODULES".to_string(), "1".to_string());
    build_env.insert("DO_NOT_STRIP_MODULES".to_string(), "1".to_string());
    build_env.insert("PAGE_SIZE".to_string(), "4096".to_string());

    if let Some(true) = proj.extra_host_env {
        let kbt = toolchain_base.join("kernel-build-tools/linux-x86");
        let sysroot = toolchain_base.join("gcc/linux-x86/host/x86_64-linux-glibc2.17-4.8/sysroot");

        build_env.insert(
            "LD_LIBRARY_PATH".to_string(),
            format!(
                "{}:{}/lib64",
                env::var("LD_LIBRARY_PATH").unwrap_or_default(),
                kbt.display()
            ),
        );

        let sysroot_flag = format!("--sysroot={} ", sysroot.display());
        let cflags = format!("-I{}/include ", kbt.display());
        let ldflags = format!(
            "-L {}/lib64 -fuse-ld=lld --rtlib=compiler-rt",
            kbt.display()
        );

        build_env.insert(
            "HOSTCFLAGS".to_string(),
            format!("{}{}", sysroot_flag, cflags),
        );
        build_env.insert(
            "HOSTLDFLAGS".to_string(),
            format!("{}{}", sysroot_flag, ldflags),
        );
    }

    if let Some(ref custom_time) = custom_build_time {
        if custom_time.starts_with('#') {
            let parts: Vec<&str> = custom_time.splitn(2, ' ').collect();
            if parts.len() == 2 {
                build_env.insert(
                    "KBUILD_BUILD_VERSION".to_string(),
                    parts[0].replace("#", ""),
                );
                build_env.insert("KBUILD_BUILD_TIMESTAMP".to_string(), parts[1].to_string());
            } else {
                build_env.insert("KBUILD_BUILD_TIMESTAMP".to_string(), custom_time.clone());
            }
        } else {
            build_env.insert("KBUILD_BUILD_TIMESTAMP".to_string(), custom_time.clone());
        }
    }

    let resukisu_setup_arg = resukisu_setup_arg
        .as_deref()
        .map(str::trim)
        .filter(|arg| !arg.is_empty())
        .unwrap_or("main");

    let setup_url = match branch.as_str() {
        "resukisu" => Some((
            "https://raw.githubusercontent.com/ReSukiSU/ReSukiSU/main/kernel/setup.sh",
            resukisu_setup_arg,
        )),
        "mksu" => Some((
            "https://raw.githubusercontent.com/5ec1cff/KernelSU/main/kernel/setup.sh",
            "-",
        )),
        "ksu" => Some((
            "https://raw.githubusercontent.com/tiann/KernelSU/main/kernel/setup.sh",
            "-",
        )),
        _ => None,
    };

    if let Some((url, arg)) = setup_url {
        let cmd = format!("curl -LSs '{}' | bash -s {}", url, arg);
        run_cmd(&["bash", "-c", &cmd], Some(&kernel_source_path), false)?;
    }

    let mut feature_suffixes = Vec::new();
    if apply_susfs {
        let susfs = proj
            .susfs
            .as_ref()
            .ok_or_else(|| anyhow!("Project {} does not define a SuSFS source", project_key))?;
        let _ = apply_susfs_overlay(&kernel_source_path, susfs, &branch)?;
        feature_suffixes.push("susfs".to_string());
    }

    if apply_bbg {
        apply_bbg_overlay(&kernel_source_path, &proj, proj.bbg.as_ref())?;
        feature_suffixes.push("bbg".to_string());
    }

    let kernel_version_cmd = if target_soc_str == "sm8850" {
        vec![
            "bash",
            "-c",
            "source ./_setup_env.sh 2>/dev/null || true && make kernelversion",
        ]
    } else {
        vec!["make", "kernelversion"]
    };

    let kernel_version = run_cmd(&kernel_version_cmd, Some(&kernel_source_path), true)?
        .unwrap_or_else(|| "unknown".to_string())
        .trim()
        .to_string();

    let short_sha = run_cmd(
        &["git", "rev-parse", "--short=12", "HEAD"],
        Some(&kernel_source_path),
        true,
    )?
    .unwrap_or_else(|| "unknown".to_string())
    .trim()
    .to_string();

    let mut make_args = vec![
        "O=out",
        "ARCH=arm64",
        "SUBARCH=arm64",
        "LLVM=1",
        "LLVM_IAS=1",
        "LD=ld.lld",
        "HOSTLD=ld.lld",
        "AR=llvm-ar",
        "NM=llvm-nm",
        "OBJCOPY=llvm-objcopy",
        "OBJDUMP=llvm-objdump",
        "OBJSIZE=llvm-size",
        "READELF=llvm-readelf",
        "STRIP=llvm-strip",
        "BINDGEN=bindgen",
    ];

    let soc_arg = format!("TARGET_SOC={}", target_soc_str);
    make_args.push(&soc_arg);

    make_args.push(&rustc_arg);
    make_args.push(&hostrustc_arg);
    make_args.push(&cc_arg);
    make_args.push(&hostcc_arg);

    fs::write(kernel_source_path.join("protected_module_names_list"), "")?;
    fs::write(kernel_source_path.join("protected_exports_list"), "")?;

    let git_exclude_path = kernel_source_path.join(".git/info/exclude");
    let mut exclude_data = fs::read_to_string(&git_exclude_path).unwrap_or_default();
    exclude_data.push_str("\nprotected_module_names_list\nprotected_exports_list\n");
    let _ = fs::write(git_exclude_path, exclude_data);

    build_env.insert("CC".to_string(), cc_cmd.clone());
    build_env.insert("HOSTCC".to_string(), cc_cmd.clone());
    build_env.insert("LD".to_string(), "ld.lld".to_string());
    build_env.insert("HOSTLD".to_string(), "ld.lld".to_string());

    let variant_suffix = match branch.as_str() {
        "main" | "lkm" => "LKM".to_string(),
        "ksu" => "KSU".to_string(),
        "mksu" => "MKSU".to_string(),
        "resukisu" | "sukisuultra" => "ReSuki".to_string(),
        _ => branch.to_uppercase(),
    };

    let mut localversion = if let Some(ref custom) = custom_localversion {
        let custom = custom.trim();
        if target_soc_str == "sm8850" {
            format!("-{}", custom.trim_start_matches('-'))
        } else {
            custom.to_string()
        }
    } else {
        format!("{}-{}", proj.localversion_base, variant_suffix)
    };

    if target_soc_str == "sm8750" {
        let sm8750_base = custom_localversion
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&localversion);
        localversion = build_sm8750_localversion(sm8750_base, &short_sha, &kernel_version)?;

        println!(
            "sm8750 uname release: {}{} (len={})",
            kernel_version,
            localversion,
            kernel_version.len() + localversion.len()
        );
    }

    if target_soc_str == "sm8850" {
        if custom_localversion.is_none() {
            localversion = format!("{}-g{}-4k", proj.localversion_base, short_sha);
        }
        let _ = fs::write(kernel_source_path.join(".scmversion"), "");
        make_args.push("LOCALVERSION_AUTO=n");
        build_env.insert("LOCALVERSION_AUTO".to_string(), "n".to_string());
        apply_sm8850_localversion(&kernel_source_path, &proj.defconfig, &localversion)?;
    }

    if target_soc_str == "sm8850" {
        let build_config_path = kernel_source_path.join("build.config.gki");
        if build_config_path.exists() {
            let content = fs::read_to_string(&build_config_path).unwrap_or_default();
            let _ = fs::write(&build_config_path, content.replace("check_defconfig", ""));
        }

        let defconfig_file =
            kernel_source_path.join(format!("arch/arm64/configs/{}", proj.defconfig));
        if defconfig_file.exists() {
            let mut defconfig_content = fs::read_to_string(&defconfig_file).unwrap_or_default();
            defconfig_content = upsert_kconfig_entry(&defconfig_content, "CONFIG_RUST", "y");
            defconfig_content =
                upsert_kconfig_entry(&defconfig_content, "CONFIG_ANDROID_BINDER_IPC_RUST", "m");
            defconfig_content = upsert_kconfig_entry(
                &defconfig_content,
                "CONFIG_CC_OPTIMIZE_FOR_PERFORMANCE",
                "y",
            );
            defconfig_content =
                upsert_kconfig_entry(&defconfig_content, "CONFIG_HEADERS_INSTALL", "n");
            defconfig_content = upsert_kconfig_entry(&defconfig_content, "CONFIG_TMPFS_XATTR", "y");
            defconfig_content =
                upsert_kconfig_entry(&defconfig_content, "CONFIG_TMPFS_POSIX_ACL", "y");
            if setup_url.is_some() {
                defconfig_content = upsert_kconfig_entry(&defconfig_content, "CONFIG_KSU", "y");
            }
            let _ = fs::write(&defconfig_file, defconfig_content);
        }
    }

    if target_soc_str == "sm8850" {
        println!("Testing Environment and rust_is_available.sh...");
        let mut cmd = std::process::Command::new("bash");
        cmd.arg("-c").arg("source ./_setup_env.sh 2>/dev/null || true && echo '=== Toolchain Versions ===' && $CC --version | head -n1 && $RUSTC -V && bindgen --version && pahole --version && echo '==========================' && sh scripts/rust_is_available.sh -v");
        cmd.current_dir(&kernel_source_path);
        for (k, v) in &build_env {
            cmd.env(k, v);
        }
        if let Ok(output) = cmd.output() {
            println!("rust_is_available.sh Exit Status: {}", output.status);
            println!("stdout:\n{}", String::from_utf8_lossy(&output.stdout));
            println!("stderr:\n{}", String::from_utf8_lossy(&output.stderr));
        } else {
            println!("Failed to execute rust_is_available.sh process.");
        }
    }

    if target_soc_str == "sm8850" {
        let mut cmd_str = format!(
            "source ./_setup_env.sh 2>/dev/null || true && make {}",
            proj.defconfig
        );
        for arg in &make_args {
            cmd_str.push_str(&format!(" '{}'", arg));
        }
        run_cmd_with_env(
            &["bash", "-c", &cmd_str],
            Some(&kernel_source_path),
            &build_env,
        )?;
    } else {
        let mut defconfig_cmd = vec!["make"];
        defconfig_cmd.extend_from_slice(&make_args);
        defconfig_cmd.push(&proj.defconfig);
        run_cmd_with_env(&defconfig_cmd, Some(&kernel_source_path), &build_env)?;
    }

    let mut disable_configs = vec!["TRIM_UNUSED_KSYMS"];
    if let Some(disables) = &proj.disable_security {
        for d in disables {
            disable_configs.push(d);
        }
    }

    for config in disable_configs {
        run_cmd(
            &[
                "scripts/config",
                "--file",
                "out/.config",
                "--disable",
                config,
            ],
            Some(&kernel_source_path),
            false,
        )?;
    }

    if let Some(lto) = &proj.lto {
        if lto == "thin" {
            run_cmd(
                &[
                    "scripts/config",
                    "--file",
                    "out/.config",
                    "-e",
                    "LTO_CLANG_THIN",
                    "-d",
                    "LTO_CLANG_FULL",
                ],
                Some(&kernel_source_path),
                false,
            )?;
        } else if lto == "full" {
            run_cmd(
                &[
                    "scripts/config",
                    "--file",
                    "out/.config",
                    "-e",
                    "LTO_CLANG_FULL",
                    "-d",
                    "LTO_CLANG_THIN",
                ],
                Some(&kernel_source_path),
                false,
            )?;
        } else if lto == "none" {
            run_cmd(
                &[
                    "scripts/config",
                    "--file",
                    "out/.config",
                    "-e",
                    "LTO_NONE",
                    "-d",
                    "LTO_CLANG_THIN",
                    "-d",
                    "LTO_CLANG_FULL",
                ],
                Some(&kernel_source_path),
                false,
            )?;
        }
    }

    if target_soc_str == "sm8850" {
        let mut cmd_str =
            "source ./_setup_env.sh 2>/dev/null || true && make olddefconfig".to_string();
        for arg in &make_args {
            cmd_str.push_str(&format!(" '{}'", arg));
        }
        run_cmd_with_env(
            &["bash", "-c", &cmd_str],
            Some(&kernel_source_path),
            &build_env,
        )?;
    } else {
        let mut olddefconfig_cmd = vec!["make"];
        olddefconfig_cmd.extend_from_slice(&make_args);
        olddefconfig_cmd.push("olddefconfig");
        run_cmd_with_env(&olddefconfig_cmd, Some(&kernel_source_path), &build_env)?;
    }

    if custom_localversion.is_some() && target_soc_str != "sm8850" {
        let _ = fs::write(kernel_source_path.join(".scmversion"), "");
        make_args.push("LOCALVERSION_AUTO=n");
        build_env.insert("LOCALVERSION_AUTO".to_string(), "n".to_string());
    }

    let localversion_arg = format!("LOCALVERSION={}", localversion);

    if target_soc_str != "sm8850" {
        if proj.version_method.as_deref().unwrap_or("param") == "file" {
            let _ = fs::write(
                kernel_source_path.join("localversion"),
                localversion.clone(),
            );
        } else {
            make_args.push(&localversion_arg);
            build_env.insert("LOCALVERSION".to_string(), localversion.clone());
        }
    } else {
        if proj.version_method.as_deref().unwrap_or("param") == "file" {
            let _ = fs::write(kernel_source_path.join("localversion"), "");
        }
        make_args.push("LOCALVERSION=");
        build_env.insert("LOCALVERSION".to_string(), "".to_string());
    }

    let threads = run_cmd(&["nproc"], None, true)?.unwrap().trim().to_string();
    let jobs = format!("-j{}", threads);

    if target_soc_str == "sm8850" {
        let mut cmd_str = format!(
            "source ./_setup_env.sh 2>/dev/null || true && make {} Image",
            jobs
        );
        for arg in &make_args {
            cmd_str.push_str(&format!(" '{}'", arg));
        }
        run_cmd_with_env(
            &["bash", "-c", &cmd_str],
            Some(&kernel_source_path),
            &build_env,
        )?;
    } else {
        let mut build_cmd = vec!["make", &jobs, "Image"];
        if target_soc_str != "sm8850" {
            build_cmd.push("modules");
        }
        build_cmd.extend_from_slice(&make_args);
        run_cmd_with_env(&build_cmd, Some(&kernel_source_path), &build_env)?;
    }

    if proj.version_method.as_deref().unwrap_or("param") == "file" {
        fs::write(kernel_source_path.join("localversion"), "")?;
    }

    let ak3_repo = proj
        .anykernel_repo
        .as_deref()
        .unwrap_or("https://github.com/YuzakiKokuban/AnyKernel3.git");
    let ak3_branch = proj.anykernel_branch.as_deref().unwrap_or("master");

    if Path::new("AnyKernel3").exists() {
        fs::remove_dir_all("AnyKernel3")?;
    }

    run_cmd(
        &["git", "clone", ak3_repo, "-b", ak3_branch, "AnyKernel3"],
        None,
        false,
    )?;

    let image_path = kernel_source_path.join("out/arch/arm64/boot/Image");
    if !image_path.exists() {
        return Err(anyhow!("Image not found at {:?}", image_path));
    }

    fs::copy(image_path, "AnyKernel3/Image")?;

    let date_str = Local::now().format("%Y%m%d-%H%M").to_string();
    let zip_prefix = proj.zip_name_prefix.as_deref().unwrap_or("Kernel");
    let feature_suffix = if feature_suffixes.is_empty() {
        String::new()
    } else {
        format!("-{}", feature_suffixes.join("-"))
    };

    let clean_localversion = localversion.trim_start_matches('-');
    let final_zip_name = format!(
        "{}-{}-{}{}-{}.zip",
        zip_prefix, kernel_version, clean_localversion, feature_suffix, date_str
    );

    run_cmd(
        &[
            "zip",
            "-r9",
            format!("../{}", final_zip_name).as_str(),
            ".",
            "-x",
            ".git*",
            "-x",
            ".github*",
            "-x",
            "README.md",
            "-x",
            "LICENSE",
            "-x",
            "*.gitignore",
            "-x",
            "patch_linux",
            "-x",
            "tools/boot.img.lz4",
            "-x",
            "tools/libmagiskboot.so",
        ],
        Some(Path::new("AnyKernel3")),
        false,
    )?;

    if do_release {
        let release_tag = format!(
            "{}-{}{}-{}",
            zip_prefix, variant_suffix, feature_suffix, date_str
        );
        let release_title = format!(
            "{} {}{} Build ({})",
            zip_prefix, variant_suffix, feature_suffix, date_str
        );

        if Path::new(&final_zip_name).exists() {
            run_cmd(
                &[
                    "gh",
                    "release",
                    "create",
                    &release_tag,
                    &final_zip_name,
                    "--repo",
                    &proj.repo,
                    "--title",
                    &release_title,
                    "--notes",
                    &format!(
                        "Automated build for {}\nKernel Version: {}",
                        branch, kernel_version
                    ),
                ],
                None,
                false,
            )?;

            handle_notify(release_tag)?;
        } else {
            return Err(anyhow!("Final zip not found"));
        }
    }

    Ok(())
}

pub fn handle_collect_artifacts(artifact_dir: String) -> Result<()> {
    let artifact_dir = PathBuf::from(artifact_dir);
    fs::create_dir_all(&artifact_dir)?;

    let mut has_artifacts = false;

    for entry in fs::read_dir(".")? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("zip") {
            has_artifacts |= copy_artifact_if_exists(&path, &artifact_dir)?;
        }
    }

    for extra_artifact in [
        "kernel_source/out/.config",
        "kernel_source/out/vmlinux.symvers",
    ] {
        has_artifacts |= copy_artifact_if_exists(Path::new(extra_artifact), &artifact_dir)?;
    }

    set_github_output(
        "has_artifacts",
        if has_artifacts { "true" } else { "false" },
    )?;

    if has_artifacts {
        println!("Collected build artifacts into {}", artifact_dir.display());
    } else {
        println!("No build artifacts were produced, skipping upload.");
    }

    Ok(())
}
