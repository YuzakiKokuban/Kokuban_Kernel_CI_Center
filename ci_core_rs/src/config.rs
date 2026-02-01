use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProjectConfig {
    pub repo: String,
    pub defconfig: String,
    pub localversion_base: String,
    pub lto: Option<String>,
    pub supported_ksu: Option<Vec<String>>,
    pub toolchain_urls: Option<Vec<String>>,
    pub toolchain_path_prefix: Option<String>,
    pub toolchain_path_exports: Option<Vec<String>>,
    pub anykernel_repo: Option<String>,
    pub anykernel_branch: Option<String>,
    pub zip_name_prefix: Option<String>,
    pub version_method: Option<String>,
    pub extra_host_env: Option<bool>,
    pub disable_security: Option<Vec<String>>,
    pub readme_placeholders: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GlobalConfig {
    pub broadcast_channel: Option<String>,
    pub resukisu_chat_id: Option<String>,
    pub resukisu_topic_id: Option<i32>,
}

pub type ProjectsMap = HashMap<String, serde_json::Value>;

pub const KSU_CONFIG_JSON: &str = r#"{
    "ksu": {
        "repo": "https://github.com/tiann/KernelSU.git",
        "branch": "main",
        "setup_url": "https://raw.githubusercontent.com/tiann/KernelSU/main/kernel/setup.sh",
        "setup_args": ["main"]
    },
    "mksu": {
        "repo": "https://github.com/5ec1cff/KernelSU.git",
        "branch": "main",
        "setup_url": "https://raw.githubusercontent.com/5ec1cff/KernelSU/main/kernel/setup.sh",
        "setup_args": ["main"]
    },
    "resukisu": {
        "repo": "https://github.com/ReSukiSU/ReSukiSU.git",
        "branch": "main",
        "setup_url": "https://raw.githubusercontent.com/ReSukiSU/ReSukiSU/main/kernel/setup.sh",
        "setup_args": ["builtin"]
    }
}"#;

#[derive(Deserialize)]
pub struct KsuConfigItem {
    pub repo: String,
    pub branch: String,
    pub setup_url: String,
    pub setup_args: Vec<String>,
}
