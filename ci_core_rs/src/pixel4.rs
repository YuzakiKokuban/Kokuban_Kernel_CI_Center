/// Pixel 4 uses a pre-GKI build and deliberately excludes SUSFS.
pub const BUILD_STYLE: &str = "pixel4-4.14";

pub fn is_pixel4(style: Option<&str>) -> bool {
    style == Some(BUILD_STYLE)
}

pub fn apply_susfs(style: Option<&str>, requested: bool) -> bool {
    requested && !is_pixel4(style)
}

pub fn localversion(base: &str, sha: &str) -> anyhow::Result<String> {
    if sha.is_empty() || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(anyhow::anyhow!("Invalid kernel source commit: {sha}"));
    }
    Ok(format!(
        "-{}-g{}-openela",
        base.trim_start_matches('-').trim_end_matches("-openela"),
        sha
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_uses_kaltsit_and_preserves_the_ci_kernel_hash() {
        assert_eq!(
            localversion("-Kokuban-Kaltsit-BakaSU", "abcdef012345").unwrap(),
            "-Kokuban-Kaltsit-BakaSU-gabcdef012345-openela"
        );
        assert_eq!(
            localversion("-Custom-openela", "abcdef012345").unwrap(),
            "-Custom-gabcdef012345-openela"
        );
        assert!(localversion("-Kokuban-Kaltsit-BakaSU", "unknown").is_err());
        assert!(localversion("-Kokuban-Kaltsit-BakaSU", "").is_err());
    }

    #[test]
    fn pixel4_matrix_builds_only_the_existing_bakasu_branch() {
        let projects: serde_json::Value =
            serde_json::from_str(include_str!("../../configs/projects.json")).unwrap();
        let project: crate::config::ProjectConfig =
            serde_json::from_value(projects["floral_sm8150"].clone()).unwrap();
        assert!(project.susfs.is_none());
        assert_eq!(project.defconfig, "floral_defconfig");
        let matrix = crate::build_project_matrix_entries("floral_sm8150", &project);
        assert_eq!(matrix.len(), 1);
        assert_eq!(matrix[0]["branch"], "bakasu");
        assert_eq!(matrix[0]["ksu_variant"], "bakasu");
        let configs: serde_json::Value =
            serde_json::from_str(include_str!("../../configs/anykernel_configs.json")).unwrap();
        let ak3: crate::config::AnyKernelConfig =
            serde_json::from_value(configs["floral"].clone()).unwrap();
        assert!(ak3.device_check && ak3.is_slot_device);
        assert_eq!(ak3.device_names, vec!["flame", "coral"]);
    }

    #[test]
    fn selects_only_the_pixel4_pipeline() {
        assert!(is_pixel4(Some(BUILD_STYLE)));
        assert!(!is_pixel4(None));
        assert!(!is_pixel4(Some("gki-6.12")));
    }

    #[test]
    fn pixel4_cannot_inherit_the_global_susfs_default() {
        assert!(!apply_susfs(Some(BUILD_STYLE), true));
        assert!(!apply_susfs(Some(BUILD_STYLE), false));
        assert!(apply_susfs(None, true));
        assert!(!apply_susfs(None, false));
    }
}
