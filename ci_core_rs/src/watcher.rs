use crate::config::{ProjectConfig, ProjectsMap};
use crate::utils::normalize_variant_name;
use anyhow::{Result, anyhow, ensure};
use std::collections::HashMap;

pub type UpdateMatrix = Vec<HashMap<String, String>>;

pub fn validate_commit(commit: &str) -> Result<()> {
    ensure!(
        commit.len() == 40 && commit.bytes().all(|c| c.is_ascii_hexdigit()),
        "Upstream commit must be a full 40-character hexadecimal SHA"
    );
    Ok(())
}

pub fn verify_installed_commit(expected: &str, actual: &str) -> Result<()> {
    validate_commit(expected)?;
    ensure!(
        expected.eq_ignore_ascii_case(actual.trim()),
        "KernelSU setup installed revision {actual:?}, expected {expected}"
    );
    Ok(())
}

pub fn pending_updates(
    projects: &ProjectsMap,
    tracker: &HashMap<String, String>,
    variant: &str,
    commit: &str,
) -> Result<UpdateMatrix> {
    validate_commit(commit)?;
    let mut matrix = Vec::new();
    for (key, value) in projects {
        if key.starts_with('_') {
            continue;
        }
        let project: ProjectConfig = serde_json::from_value(value.clone())?;
        let supported = project
            .supported_ksu
            .or(project.watch_upstream_variants)
            .unwrap_or_default();
        if supported
            .iter()
            .any(|name| normalize_variant_name(name) == variant)
            && tracker.get(&format!("{key}:{variant}")).map(String::as_str) != Some(commit)
        {
            matrix.push(HashMap::from([
                ("project".into(), key.clone()),
                ("variant".into(), variant.into()),
                ("commit_id".into(), commit.into()),
            ]));
        }
    }
    matrix.sort_by(|a, b| a["project"].cmp(&b["project"]));
    Ok(matrix)
}

pub fn acknowledge(tracker: &mut HashMap<String, String>, matrix: &UpdateMatrix) -> Result<()> {
    // Validate the complete wave before mutating any success record.
    let mut records = Vec::new();
    for entry in matrix {
        let field = |key: &str| {
            entry
                .get(key)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("Missing wave field: {key}"))
        };
        let project = field("project")?;
        let variant = field("variant")?;
        let commit = field("commit_id")?;
        validate_commit(commit)?;
        records.push((format!("{project}:{variant}"), commit.clone()));
    }
    tracker.extend(records);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projects() -> crate::config::ProjectsMap {
        serde_json::from_value(serde_json::json!({
            "existing": {"repo":"owner/kernel", "defconfig":"vendor/test", "localversion_base":"-Test", "supported_ksu":["bakasu"]},
            "new": {"repo":"owner/new", "defconfig":"vendor/new", "localversion_base":"-New", "watch_upstream_variants":["bakasu"]}
        })).unwrap()
    }

    #[test]
    fn setup_must_install_the_requested_revision() {
        let expected = "a".repeat(40);
        assert!(verify_installed_commit(&expected, &expected).is_ok());
        assert!(verify_installed_commit(&expected, &"b".repeat(40)).is_err());
        assert!(verify_installed_commit(&expected, "").is_err());
    }

    #[test]
    fn legacy_global_record_does_not_hide_new_projects() {
        let sha = "a".repeat(40);
        let tracker = HashMap::from([("bakasu".into(), sha.clone())]);
        let matrix = pending_updates(&projects(), &tracker, "bakasu", &sha).unwrap();
        assert_eq!(matrix.len(), 2);
        assert!(matrix.iter().all(|entry| entry["commit_id"] == sha));
    }

    #[test]
    fn failed_wave_remains_pending_until_acknowledged() {
        let sha = "a".repeat(40);
        let mut tracker = HashMap::new();
        let first = pending_updates(&projects(), &tracker, "bakasu", &sha).unwrap();
        assert_eq!(
            pending_updates(&projects(), &tracker, "bakasu", &sha).unwrap(),
            first
        );
        acknowledge(&mut tracker, &first).unwrap();
        assert!(
            pending_updates(&projects(), &tracker, "bakasu", &sha)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            pending_updates(&projects(), &tracker, "bakasu", &"b".repeat(40))
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn adding_project_after_success_requires_backfill() {
        let sha = "a".repeat(40);
        let tracker = HashMap::from([("existing:bakasu".into(), sha.clone())]);
        let matrix = pending_updates(&projects(), &tracker, "bakasu", &sha).unwrap();
        assert_eq!(matrix.len(), 1);
        assert_eq!(matrix[0]["project"], "new");
    }

    #[test]
    fn rejects_empty_or_abbreviated_upstream_hashes() {
        for sha in [
            "",
            "abcdef0",
            "main",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaag",
        ] {
            assert!(pending_updates(&projects(), &HashMap::new(), "bakasu", sha).is_err());
        }
    }

    #[test]
    fn invalid_acknowledgement_does_not_partially_update_tracker() {
        let mut tracker = HashMap::new();
        let matrix = vec![
            HashMap::from([
                ("project".into(), "existing".into()),
                ("variant".into(), "bakasu".into()),
                ("commit_id".into(), "a".repeat(40)),
            ]),
            HashMap::from([("project".into(), "new".into())]),
        ];
        assert!(acknowledge(&mut tracker, &matrix).is_err());
        assert!(tracker.is_empty());
    }
}
