use std::path::{Path, PathBuf};
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
    embed_marketplace_catalog();
}

/// Embeds every file under `marketplace/coppice/` as `(path relative to the
/// catalog root, bytes)`, so the first-party catalog ships in the binary
/// without a runtime checkout.
fn embed_marketplace_catalog() {
    println!("cargo:rerun-if-changed=marketplace/coppice");
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest_dir.join("marketplace/coppice");
    let mut files = Vec::new();
    collect_files(&root, &root, &mut files);
    files.sort();
    let mut out = String::from("pub const MARKETPLACE_FILES: &[(&str, &[u8])] = &[\n");
    for rel in &files {
        println!("cargo:rerun-if-changed=marketplace/coppice/{rel}");
        out.push_str(&format!(
            "    ({rel:?}, include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/marketplace/coppice/{rel}\"))),\n"
        ));
    }
    out.push_str("];\n");
    let dest = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("marketplace_catalog.rs");
    std::fs::write(dest, out).unwrap();
}

fn collect_files(root: &Path, dir: &Path, files: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Symlinks are skipped so the catalog cannot embed files from outside it.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            collect_files(root, &path, files);
        } else if file_type.is_file() {
            if let Ok(rel) = path.strip_prefix(root) {
                files.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
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
