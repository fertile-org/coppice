//! Gateway names for plugin tools: `<plugin>__<tool>`, at most 50 chars so a
//! client prefix like `mcp__coppice__` stays within the 64-char tool name limit.

use sha2::{Digest, Sha256};

pub const MAX_EXPOSED_LEN: usize = 50;
const KEPT_PREFIX: usize = 41;
const HASH_HEX: usize = 8;

pub fn exposed_name(plugin: &str, tool: &str) -> String {
    let name = format!("{}__{}", sanitize(plugin), sanitize(tool));
    if name.len() <= MAX_EXPOSED_LEN {
        return name;
    }
    let digest = Sha256::digest(format!("{plugin}__{tool}").as_bytes());
    let hash = hex::encode(digest);
    format!("{}_{}", &name[..KEPT_PREFIX], &hash[..HASH_HEX])
}

/// Output is ASCII, so byte slicing above stays on char boundaries.
fn sanitize(part: &str) -> String {
    part.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposed_name_basic_and_sanitized() {
        assert_eq!(
            exposed_name("github", "create_issue"),
            "github__create_issue"
        );
        assert_eq!(exposed_name("my.plugin", "do it"), "my_plugin__do_it");
        assert_eq!(exposed_name("a-b", "c_d"), "a-b__c_d");
    }

    #[test]
    fn exposed_name_long_is_hashed_to_50() {
        let plugin = "p".repeat(30);
        let tool = "t".repeat(28);
        let name = exposed_name(&plugin, &tool);
        let full = format!("{plugin}__{tool}");
        assert_eq!(full.len(), 60);
        assert_eq!(name.len(), MAX_EXPOSED_LEN);
        assert_eq!(&name[..41], &full[..41]);
        assert_eq!(&name[41..42], "_");
        let hash = &name[42..];
        assert_eq!(hash.len(), 8);
        assert!(hash
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));

        let other = exposed_name(&plugin, &format!("{}x", "t".repeat(27)));
        assert_eq!(&other[..41], &name[..41]);
        assert_ne!(other, name);
    }
}
