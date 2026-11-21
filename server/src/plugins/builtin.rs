use std::io;
use std::path::Path;

/// Built-in plugin name; skills live under `<dir>/coppice/skills/<id>/SKILL.md`.
pub const BUILTIN_PLUGIN: &str = "coppice";

pub const BUILTIN_SKILLS: &[(&str, &str)] = &[
    (
        "coppice-collaboration",
        include_str!("../../builtin-plugins/coppice/skills/coppice-collaboration/SKILL.md"),
    ),
    (
        "coppice-splitting",
        include_str!("../../builtin-plugins/coppice/skills/coppice-splitting/SKILL.md"),
    ),
    (
        "coppice-pm-refinement",
        include_str!("../../builtin-plugins/coppice/skills/coppice-pm-refinement/SKILL.md"),
    ),
    (
        "coppice-tech-lead-review",
        include_str!("../../builtin-plugins/coppice/skills/coppice-tech-lead-review/SKILL.md"),
    ),
    (
        "coppice-qc-verification",
        include_str!("../../builtin-plugins/coppice/skills/coppice-qc-verification/SKILL.md"),
    ),
    (
        "coppice-git",
        include_str!("../../builtin-plugins/coppice/skills/coppice-git/SKILL.md"),
    ),
];

const DESIGN_PLUGIN_JSON: &str =
    include_str!("../../builtin-plugins/design/.claude-plugin/plugin.json");
const DESIGN_UI_SKILL: &str =
    include_str!("../../builtin-plugins/design/skills/design-ui-options/SKILL.md");
const DESIGN_TECHNICAL_SKILL: &str =
    include_str!("../../builtin-plugins/design/skills/design-technical-options/SKILL.md");

/// How agents load the UI design skill: `<plugin>:<skill>`.
pub const DESIGN_SKILL_ID: &str = "design:design-ui-options";

/// How agents load the technical options skill: `<plugin>:<skill>`.
pub const DESIGN_TECHNICAL_SKILL_ID: &str = "design:design-technical-options";

/// Writes the embedded built-in skills to `<dir>/coppice/skills/<name>/SKILL.md`.
/// The `coppice/skills` subtree is replaced wholesale so an upgrade both
/// refreshes and prunes skills; the rest of `dir` is left alone.
pub fn materialize_builtin(dir: &Path) -> io::Result<()> {
    let skills_root = dir.join(BUILTIN_PLUGIN).join("skills");
    match std::fs::remove_dir_all(&skills_root) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }
    for (name, contents) in BUILTIN_SKILLS {
        let skill_dir = skills_root.join(name);
        std::fs::create_dir_all(&skill_dir)?;
        std::fs::write(skill_dir.join("SKILL.md"), contents)?;
    }
    let design_root = dir.join("design");
    match std::fs::remove_dir_all(&design_root) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }
    let manifest_dir = design_root.join(".claude-plugin");
    std::fs::create_dir_all(&manifest_dir)?;
    std::fs::write(manifest_dir.join("plugin.json"), DESIGN_PLUGIN_JSON)?;
    for (name, contents) in [
        ("design-ui-options", DESIGN_UI_SKILL),
        ("design-technical-options", DESIGN_TECHNICAL_SKILL),
    ] {
        let skill_dir = design_root.join("skills").join(name);
        std::fs::create_dir_all(&skill_dir)?;
        std::fs::write(skill_dir.join("SKILL.md"), contents)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn design_skill_ids_match_the_shipped_skills() {
        for (id, body) in [
            (DESIGN_SKILL_ID, DESIGN_UI_SKILL),
            (DESIGN_TECHNICAL_SKILL_ID, DESIGN_TECHNICAL_SKILL),
        ] {
            let (plugin, name) = id.split_once(':').unwrap();
            assert_eq!(plugin, "design");
            assert!(body.contains(&format!("name: {name}\n")), "{id}");
        }
    }

    #[test]
    fn materialize_builtin_writes_both_design_skills() {
        let dir = tempfile::tempdir().unwrap();
        materialize_builtin(dir.path()).unwrap();
        assert!(dir
            .path()
            .join("design/skills/design-ui-options/SKILL.md")
            .is_file());
        assert!(dir
            .path()
            .join("design/skills/design-technical-options/SKILL.md")
            .is_file());
        assert!(dir
            .path()
            .join("design/.claude-plugin/plugin.json")
            .is_file());
    }
}
