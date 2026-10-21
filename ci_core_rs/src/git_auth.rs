pub fn github_clone_url(repo: &str, token: Option<&str>) -> String {
    if let Some(token) = token {
        // GitHub accepts the token as the HTTPS password. A token-only username
        // leaves Git prompting for credentials when a push requires authentication.
        format!("https://x-access-token:{token}@github.com/{repo}.git")
    } else {
        format!("https://github.com/{repo}.git")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Command, Stdio};

    #[test]
    fn token_authentication_needs_no_interactive_credentials() {
        let url = github_clone_url("owner/kernel", Some("test-token"));
        let mut child = Command::new("git")
            .args([
                "-c",
                "credential.helper=",
                "-c",
                "core.askPass=",
                "credential",
                "fill",
            ])
            .env("GIT_TERMINAL_PROMPT", "0")
            .env_remove("GIT_ASKPASS")
            .env_remove("SSH_ASKPASS")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(child.stdin.take().unwrap(), "url={url}\n").unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "Git required interactive credentials: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let credentials = String::from_utf8(output.stdout).unwrap();
        assert!(
            credentials
                .lines()
                .any(|line| line == "username=x-access-token")
        );
        assert!(
            credentials
                .lines()
                .any(|line| line == "password=test-token")
        );
    }
}
