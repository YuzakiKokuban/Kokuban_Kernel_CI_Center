use std::process::Command;

/// Stamp the binary with the exact `ci_core_rs` tree it was compiled from.
///
/// This core is published to a *moving* `ci-core-latest` release, and every kernel build
/// downloads whatever happens to be there. A kernel run that starts while a new core is still
/// uploading therefore executes the previous core, and every check added in the new one is
/// silently absent rather than failing -- which is exactly how the consumer-side ABI gate went
/// missing from a build that was supposed to run it. A revision alone would not be enough,
/// because a core built from a later commit that did not touch this directory is still fresh;
/// the tree hash depends only on the content, so the downloader can prove what it got.
fn main() {
    // Only `src` matters: the stamp is the tree hash of this whole directory, so a commit
    // that leaves `ci_core_rs` untouched leaves the stamp correct even if the binary is
    // rebuilt, and no other input can change the value.
    println!("cargo:rerun-if-changed=src");

    // Cargo runs build scripts with the manifest directory as the working directory, but
    // deriving it from the manifest path keeps the lookup correct if that ever changes.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let repo_root = std::path::Path::new(&manifest_dir)
        .parent()
        .map(|path| path.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let tree = Command::new("git")
        .args(["rev-parse", "HEAD:ci_core_rs"])
        .current_dir(&repo_root)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=KOKUBAN_CORE_TREE={tree}");
}
