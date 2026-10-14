use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=COPPICE_BUILD_COMMIT");
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs/heads");
    let commit = std::env::var("COPPICE_BUILD_COMMIT")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(git_head);
    println!("cargo:rustc-env=COPPICE_BUILD_COMMIT={commit}");
}

fn git_head() -> String {
    let output = Command::new("git").args(["rev-parse", "HEAD"]).output();
    match output {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if text.is_empty() {
                "unknown".into()
            } else {
                text
            }
        }
        _ => "unknown".into(),
    }
}
