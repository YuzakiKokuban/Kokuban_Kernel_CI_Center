use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProjectConfig {
    pub repo: String,
    pub defconfig: String,
    pub localversion_base: String,
    pub expected_kernel_version: Option<String>,
    pub lto: Option<String>,
    pub supported_ksu: Option<Vec<String>>,
    pub toolchain_urls: Option<Vec<String>>,
    pub toolchain_sha256: Option<HashMap<String, String>>,
    pub toolchain_path_prefix: Option<String>,
    pub toolchain_path_exports: Option<Vec<String>>,
    pub anykernel_config: Option<String>,
    pub zip_name_prefix: Option<String>,
    pub version_method: Option<String>,
    /// Selects the build pipeline for trees that are not plain `make` kernels.
    /// `"gki-6.12"` opts into the ACK/GKI path shared with `mi17_sm8850`.
    pub build_style: Option<String>,
    /// Exported-symbol CRCs the built `vmlinux` must reproduce exactly, as
    /// `symbol -> "0x…"`. A mismatch means the stock vendor modules will refuse
    /// to load, so the build fails instead of shipping a bootlooping image.
    pub abi_symbol_gates: Option<HashMap<String, String>>,
    /// Path inside the kernel source of a `CONFIG_x=y` file overlaid onto the
    /// defconfig just before configuration, so the delta stays reviewable in one
    /// small file instead of being buried in a full `.config`.
    pub kconfig_fragment: Option<String>,
    pub extra_host_env: Option<bool>,
    pub disable_security: Option<Vec<String>>,
    pub readme_placeholders: Option<HashMap<String, String>>,
    pub susfs: Option<SusfsConfig>,
    pub bbg: Option<BbgConfig>,
    pub watch_upstream_variants: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SusfsConfig {
    pub repo: String,
    pub branch: String,
    pub patch_path: String,
    pub fs_patch_dir: Option<String>,
    pub include_linux_patch_dir: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BbgConfig {
    pub setup_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GlobalConfig {
    pub broadcast_channel: Option<String>,
    pub resukisu_chat_id: Option<String>,
    pub resukisu_topic_id: Option<i32>,
}

pub type ProjectsMap = HashMap<String, serde_json::Value>;
pub type AnyKernelConfigMap = HashMap<String, AnyKernelConfig>;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AnyKernelConfig {
    pub kernel_string: String,
    pub device_check: bool,
    pub modules: bool,
    pub systemless: bool,
    pub cleanup: bool,
    pub cleanup_on_abort: bool,
    pub device_names: Vec<String>,
    pub supported_versions: Option<String>,
    pub supported_patchlevels: Option<String>,
    pub supported_vendorpatchlevels: Option<String>,
    pub block: String,
    pub is_slot_device: bool,
    pub ramdisk_compression: Option<String>,
    pub patch_vbmeta_flag: Option<String>,
    pub boot_setup: Option<String>,
    pub boot_finalize: Option<String>,
}

pub const KSU_CONFIG_JSON: &str = r#"{
    "resukisu": {
        "repo": "https://github.com/ReSukiSU/ReSukiSU.git",
        "branch": "main",
        "setup_url": "https://raw.githubusercontent.com/ReSukiSU/ReSukiSU/main/kernel/setup.sh",
        "setup_args": ["main"]
    }
}"#;

#[derive(Deserialize)]
pub struct KsuConfigItem {
    pub repo: String,
    pub branch: String,
    pub setup_url: String,
    pub setup_args: Vec<String>,
}
