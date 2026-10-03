use std::path::{Path, PathBuf};

const SKILL_MD: &str = "SKILL.md";

pub const SKILL_CONTAINERS: [&str; 6] = [
    "skills",
    "skills/.curated",
    "skills/.experimental",
    "skills/.system",
    ".agents/skills",
    ".claude/skills",
];

/// Rel paths (from `root`, sorted) of dirs holding `SKILL.md`, at most
/// `max_depth` levels below `container_rel`; `root` must be canonical.
pub fn skill_dirs(root: &Path, container_rel: &str, max_depth: usize) -> Vec<String> {
    walk(root, container_rel, max_depth).skills
}

/// `root` has a `SKILL.md`, or a container (3 levels) or `root` (1 level) holds a skill.
pub fn has_skills_package(root: &Path) -> bool {
    let Ok(root) = std::fs::canonicalize(root) else {
        return false;
    };
    has_contained_skills(&root) || !skill_dirs(&root, "", 1).is_empty()
}

/// Like [`has_skills_package`] without the one-level search of `root` itself.
pub fn has_contained_skills(root: &Path) -> bool {
    let Ok(root) = std::fs::canonicalize(root) else {
        return false;
    };
    root.join(SKILL_MD).is_file()
        || SKILL_CONTAINERS
            .iter()
            .any(|c| !skill_dirs(&root, c, 3).is_empty())
}

#[derive(Default)]
pub(crate) struct Walk {
    pub skills: Vec<String>,
    /// Skill dirs reached through a symlink that resolves outside `root`.
    pub escaped: Vec<String>,
}

pub(crate) fn walk(root: &Path, container_rel: &str, max_depth: usize) -> Walk {
    let container_rel = container_rel.trim_end_matches('/');
    let mut walk = Walk::default();
    let Ok(container) = std::fs::canonicalize(root.join(container_rel)) else {
        return walk;
    };
    if max_depth == 0 || !container.is_dir() || !container.starts_with(root) {
        return walk;
    }
    let mut path = vec![container.clone()];
    visit(
        root,
        &container,
        container_rel,
        max_depth,
        &mut path,
        &mut walk,
    );
    walk.skills.sort();
    walk.escaped.sort();
    walk
}

/// `path` holds the canonical dirs on the current walk path, so a symlink back
/// to one of them (or an ancestor) is never followed.
fn visit(
    root: &Path,
    dir: &Path,
    rel: &str,
    depth: usize,
    path: &mut Vec<PathBuf>,
    walk: &mut Walk,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if name.starts_with('.') || name == "node_modules" {
            continue;
        }
        let Ok(canonical) = std::fs::canonicalize(dir.join(&name)) else {
            continue;
        };
        if !canonical.is_dir() {
            continue;
        }
        let child_rel = if rel.is_empty() {
            name
        } else {
            format!("{rel}/{name}")
        };
        let has_skill = canonical.join(SKILL_MD).is_file();
        if !canonical.starts_with(root) {
            if has_skill {
                walk.escaped.push(child_rel);
            }
            continue;
        }
        if path.iter().any(|p| p.starts_with(&canonical)) {
            continue;
        }
        if has_skill {
            walk.skills.push(child_rel);
        } else if depth > 1 {
            path.push(canonical.clone());
            visit(root, &canonical, &child_rel, depth - 1, path, walk);
            path.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        std::fs::canonicalize(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../fixtures/plugins")
                .join(name),
        )
        .unwrap()
    }

    fn touch_skill(path: &Path) {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join("SKILL.md"), "---\nname: x\ndescription: x\n---\n").unwrap();
    }

    #[test]
    fn walker_finds_flat_and_catalog_layouts() {
        let root = fixture("skills-catalog");
        assert_eq!(
            skill_dirs(&root, "skills", 3),
            ["skills/docs/pdf", "skills/web/design"]
        );
        assert_eq!(skill_dirs(&fixture("skills-only"), "", 1), ["review"]);
    }

    #[test]
    fn walker_stops_at_depth_limit() {
        let root = fixture("skills-catalog");
        let found = skill_dirs(&root, "skills", 3);
        assert!(!found.iter().any(|d| d == "skills/a/b/c/d"), "{found:?}");
        assert_eq!(skill_dirs(&root, "skills", 1), Vec::<String>::new());
    }

    #[test]
    fn walker_skips_dot_dirs_and_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        touch_skill(&root.join("skills/node_modules/junk"));
        touch_skill(&root.join("skills/.hidden/x"));
        touch_skill(&root.join("skills/ok"));
        touch_skill(&root.join("node_modules"));
        assert_eq!(skill_dirs(&root, "skills", 3), ["skills/ok"]);
        assert_eq!(skill_dirs(&root, "", 1), Vec::<String>::new());
    }

    #[cfg(unix)]
    #[test]
    fn walker_ignores_symlink_loops_and_escapes() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        touch_skill(&root.join("skills/x"));
        let outside = tempfile::tempdir().unwrap();
        touch_skill(&outside.path().join("leak"));
        std::os::unix::fs::symlink("..", root.join("skills/loop")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("skills/out")).unwrap();
        assert_eq!(skill_dirs(&root, "skills", 3), ["skills/x"]);
    }

    #[test]
    fn has_skills_package_cases() {
        assert!(has_skills_package(&fixture("single-skill")));
        assert!(has_skills_package(&fixture("skills-catalog")));
        assert!(has_skills_package(&fixture("skills-only")));
        let dir = tempfile::tempdir().unwrap();
        assert!(!has_skills_package(dir.path()));
        std::fs::create_dir_all(dir.path().join("skills/empty")).unwrap();
        assert!(!has_skills_package(dir.path()));
    }
}
