use crate::config::SusfsConfig;
use crate::utils::is_bakasu_variant;
use anyhow::{Result, anyhow};

pub fn resolve(
    requested: Option<bool>,
    variant: &str,
    config: Option<&SusfsConfig>,
) -> Result<bool> {
    let enabled = requested.unwrap_or_else(|| {
        is_bakasu_variant(variant) && config.is_some_and(|config| config.default_enabled)
    });
    if enabled && !is_bakasu_variant(variant) {
        return Err(anyhow!(
            "SUSFS requires the bakasu build variant; LKM is unsupported"
        ));
    }
    if enabled && config.is_none() {
        return Err(anyhow!("This project has no SUSFS patchset configured"));
    }
    Ok(enabled)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> SusfsConfig {
        serde_json::from_value(serde_json::json!({
            "repo": "https://example.com/susfs.git", "revision": "a".repeat(40),
            "patch": "kernel_patches/feature.patch", "default_enabled": true
        }))
        .unwrap()
    }

    #[test]
    fn defaults_are_project_and_variant_specific() {
        assert!(resolve(None, "bakasu", Some(&config())).unwrap());
        assert!(!resolve(None, "main", Some(&config())).unwrap());
        assert!(!resolve(None, "bakasu", None).unwrap());
        assert!(!resolve(Some(false), "bakasu", Some(&config())).unwrap());
    }

    #[test]
    fn rejects_lkm_and_missing_patchsets() {
        assert!(resolve(Some(true), "main", Some(&config())).is_err());
        assert!(resolve(Some(true), "bakasu", None).is_err());
    }
}
