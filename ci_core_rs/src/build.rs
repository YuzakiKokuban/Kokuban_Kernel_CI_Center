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

    if let Some(urls) = &proj.toolchain_urls {
        let tc_download_dir = PathBuf::from("toolchain_download");

        if tc_download_dir.exists() {
            fs::remove_dir_all(&tc_download_dir)?;
        }
        fs::create_dir_all(&tc_download_dir)?;

        for url in urls {
            run_cmd(&["wget", "-q", url], Some(&tc_download_dir), false)?;
        }

        let extract_script = r#"
            set -e
            if ls *.tar.gz.[0-9]* 1> /dev/null 2>&1; then
                cat *.tar.gz.* | tar -zxf - --warning=no-unknown-keyword -C ..
            elif ls *part_aa* 1> /dev/null 2>&1 || ls *_aa.tar.gz 1> /dev/null 2>&1 || ls *.tar.gz.aa 1> /dev/null 2>&1; then
                cat *.tar.gz | tar -zxf - --warning=no-unknown-keyword -C ..
            elif ls *.tar.gz 1> /dev/null 2>&1; then
                for tarball in *.tar.gz; do
                    tar -zxf "$tarball" --warning=no-unknown-keyword -C ..
                done
            fi
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
    build_env.insert("KCFLAGS".to_string(), "-O2 -pipe -Wno-error".to_string());
    build_env.insert("KCPPFLAGS".to_string(), "-O2 -pipe -Wno-error".to_string());

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

    let kernel_version = run_cmd(&["make", "kernelversion"], Some(&kernel_source_path), true)?
        .unwrap_or_else(|| "unknown".to_string())
        .trim()
        .to_string();

    let target_soc = project_key.split('_').nth(1).unwrap_or("unknown");
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

    let soc_arg = format!("TARGET_SOC={}", target_soc);
    make_args.push(&soc_arg);

    fs::write(kernel_source_path.join("protected_module_names_list"), "")?;
    fs::write(kernel_source_path.join("protected_exports_list"), "")?;

    let git_exclude_path = kernel_source_path.join(".git/info/exclude");
    let mut exclude_data = fs::read_to_string(&git_exclude_path).unwrap_or_default();
    exclude_data.push_str("\nprotected_module_names_list\nprotected_exports_list\n");
    let _ = fs::write(git_exclude_path, exclude_data);

    if run_cmd(&["which", "ccache"], None, false).is_ok() {
        build_env.insert("CC".to_string(), "ccache clang".to_string());
        build_env.insert("CXX".to_string(), "ccache clang++".to_string());
        build_env.insert("HOSTCC".to_string(), "ccache clang".to_string());
        build_env.insert("HOSTCXX".to_string(), "ccache clang++".to_string());
        build_env.insert(
            "CCACHE_DIR".to_string(),
            format!("{}/.ccache", env::current_dir()?.display()),
        );
        run_cmd(&["ccache", "-M", "5G"], None, false)?;
        make_args.push("CC=ccache clang");
        make_args.push("HOSTCC=ccache clang");
    } else {
        make_args.push("CC=clang");
        make_args.push("HOSTCC=clang");
    }

    let mut defconfig_cmd = vec!["make"];
    defconfig_cmd.extend_from_slice(&make_args);
    defconfig_cmd.push(&proj.defconfig);

    run_cmd_with_env(&defconfig_cmd, Some(&kernel_source_path), &build_env)?;

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
        }
    }

    let short_sha = run_cmd(
        &["git", "rev-parse", "--short", "HEAD"],
        Some(&kernel_source_path),
        true,
    )?
    .unwrap_or_else(|| "unknown".to_string());

    let variant_suffix = match branch.as_str() {
        "main" | "lkm" => "LKM".to_string(),
        "ksu" => "KSU".to_string(),
        "mksu" => "MKSU".to_string(),
        "resukisu" | "sukisuultra" => "ReSuki".to_string(),
        _ => branch.to_uppercase(),
    };

    let localversion = if let Some(custom) = custom_localversion {
        custom
    } else {
        format!("{}-{}", proj.localversion_base, variant_suffix)
    };

    if proj.version_method.as_deref().unwrap_or("param") == "file" {
        fs::write(
            kernel_source_path.join("localversion"),
            localversion.clone(),
        )?;
    } else {
        make_args.push("LOCALVERSION=");
        build_env.insert("LOCALVERSION".to_string(), localversion.clone());
    }

    let threads = run_cmd(&["nproc"], None, true)?.unwrap().trim().to_string();
    let jobs = format!("-j{}", threads);

    let mut build_cmd = vec!["make", &jobs];
    build_cmd.extend_from_slice(&make_args);

    run_cmd_with_env(&build_cmd, Some(&kernel_source_path), &build_env)?;

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

    let date_str = if let Some(custom) = custom_build_time {
        custom
    } else {
        Local::now().format("%Y%m%d-%H%M").to_string()
    };
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
