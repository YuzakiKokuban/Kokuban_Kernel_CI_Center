use std::num::{NonZeroUsize, ParseIntError};

pub fn make_jobs_arg(
    override_jobs: Option<&str>,
    detected_cpus: &str,
) -> Result<String, ParseIntError> {
    let value = override_jobs
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(detected_cpus.trim());
    let jobs: NonZeroUsize = value.parse()?;
    Ok(format!("-j{jobs}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_parallelism_overrides_detected_cpus() {
        assert_eq!(make_jobs_arg(Some(" 16 "), "4\n").unwrap(), "-j16");
    }

    #[test]
    fn missing_or_blank_override_uses_detected_cpus() {
        for value in [None, Some(""), Some(" \t ")] {
            assert_eq!(make_jobs_arg(value, "4\n").unwrap(), "-j4");
        }
    }

    #[test]
    fn rejects_unbounded_invalid_and_shell_arguments() {
        for value in [
            "0",
            "-1",
            "1.5",
            "abc",
            "8 --help",
            "8; echo unsafe",
            "999999999999999999999999999999999999",
        ] {
            assert!(
                make_jobs_arg(Some(value), "4").is_err(),
                "accepted {value:?}"
            );
        }
        for value in ["", "0", "not-a-number"] {
            assert!(make_jobs_arg(None, value).is_err());
        }
    }
}
