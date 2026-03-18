use anyhow::{Result, anyhow};
use chrono::Local;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::ProjectConfig;
use crate::utils::{handle_notify, load_projects, run_cmd, run_cmd_with_env};

pub fn handle_build(
    project_key: String,
    branch: String,
    do_release: bool,
    custom_localversion: Option<String>,
    custom_build_time: Option<String>,
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

    build_env.insert("RUSTC".to_string(), "rustc".to_string());
    build_env.insert("HOSTRUSTC".to_string(), "rustc".to_string());
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

    let setup_url = match branch.as_str() {
        "resukisu" => Some((
            "https://raw.githubusercontent.com/ReSukiSU/ReSukiSU/main/kernel/setup.sh",
            "builtin",
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

    if target_soc_str == "sm8850" {
        let setlocalversion_path = kernel_source_path.join("scripts/setlocalversion");
        if setlocalversion_path.exists() {
            let content = fs::read_to_string(&setlocalversion_path).unwrap_or_default();
            let new_content = content
                .replace(" -dirty", "")
                .replace("res=\"$res+\"", "res=\"$res\"");
            let _ = fs::write(&setlocalversion_path, new_content);
        }
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
        "RUSTC=rustc",
        "HOSTRUSTC=rustc",
        "BINDGEN=bindgen",
    ];

    let soc_arg = format!("TARGET_SOC={}", target_soc_str);
    make_args.push(&soc_arg);

    fs::write(kernel_source_path.join("protected_module_names_list"), "")?;
    fs::write(kernel_source_path.join("protected_exports_list"), "")?;

    let git_exclude_path = kernel_source_path.join(".git/info/exclude");
    let mut exclude_data = fs::read_to_string(&git_exclude_path).unwrap_or_default();
    exclude_data.push_str("\nprotected_module_names_list\nprotected_exports_list\n");
    let _ = fs::write(git_exclude_path, exclude_data);

    make_args.push("CC=clang");
    make_args.push("HOSTCC=clang");
    build_env.insert("CC".to_string(), "clang".to_string());
    build_env.insert("HOSTCC".to_string(), "clang".to_string());
    build_env.insert("LD".to_string(), "ld.lld".to_string());
    build_env.insert("HOSTLD".to_string(), "ld.lld".to_string());

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
            defconfig_content.push_str("\nCONFIG_RUST=y\n");
            defconfig_content.push_str("CONFIG_ANDROID_BINDER_IPC_RUST=m\n");
            defconfig_content.push_str("CONFIG_CC_OPTIMIZE_FOR_PERFORMANCE=y\n");
            defconfig_content.push_str("CONFIG_HEADERS_INSTALL=n\n");
            defconfig_content.push_str("CONFIG_TMPFS_XATTR=y\n");
            defconfig_content.push_str("CONFIG_TMPFS_POSIX_ACL=y\n");
            defconfig_content.push_str("CONFIG_LOCALVERSION=\"\"\n");
            defconfig_content.push_str("CONFIG_LOCALVERSION_AUTO=n\n");
            if setup_url.is_some() {
                defconfig_content.push_str("CONFIG_KSU=y\n");
            }
            let _ = fs::write(&defconfig_file, defconfig_content);
        }
    }

    if target_soc_str == "sm8850" {
        println!("Testing rust_is_available.sh...");
        let mut cmd = std::process::Command::new("bash");
        cmd.arg("-c").arg(
            "source ./_setup_env.sh 2>/dev/null || true && sh scripts/rust_is_available.sh -v",
        );
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

    let mut disable_configs = vec![
        "UH",
        "RKP",
        "KDP",
        "SECURITY_DEFEX",
        "INTEGRITY",
        "FIVE",
        "TRIM_UNUSED_KSYMS",
    ];
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

    let short_sha = run_cmd(
        &["git", "rev-parse", "--short=12", "HEAD"],
        Some(&kernel_source_path),
        true,
    )?
    .unwrap_or_else(|| "unknown".to_string())
    .trim()
    .to_string();

    let variant_suffix = match branch.as_str() {
        "main" | "lkm" => "LKM".to_string(),
        "ksu" => "KSU".to_string(),
        "mksu" => "MKSU".to_string(),
        "resukisu" | "sukisuultra" => "ReSuki".to_string(),
        _ => branch.to_uppercase(),
    };

    let mut localversion = if let Some(ref custom) = custom_localversion {
        let _ = fs::write(kernel_source_path.join(".scmversion"), "");
        make_args.push("LOCALVERSION_AUTO=n");
        build_env.insert("LOCALVERSION_AUTO".to_string(), "n".to_string());
        custom.clone()
    } else {
        format!("{}-{}", proj.localversion_base, variant_suffix)
    };

    if target_soc_str == "sm8850" {
        if custom_localversion.is_none() {
            localversion = format!("{}-g{}-4k", proj.localversion_base, short_sha);
        }
        let _ = fs::write(kernel_source_path.join(".scmversion"), "");
        make_args.push("LOCALVERSION_AUTO=n");
        build_env.insert("LOCALVERSION_AUTO".to_string(), "n".to_string());

        let setlocalversion_path = kernel_source_path.join("scripts/setlocalversion");
        if setlocalversion_path.exists() {
            let script_content = format!("#!/bin/sh\necho \"{}\"\n", localversion);
            let _ = fs::write(&setlocalversion_path, script_content);
        }
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

    let clean_localversion = localversion.trim_start_matches('-');
    let final_zip_name = format!(
        "{}-{}-{}-{}.zip",
        zip_prefix, kernel_version, clean_localversion, date_str
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
        let release_tag = format!("{}-{}-{}", zip_prefix, variant_suffix, date_str);
        let release_title = format!("{} {} Build ({})", zip_prefix, variant_suffix, date_str);

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
