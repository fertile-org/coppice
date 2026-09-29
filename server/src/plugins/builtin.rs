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
    Ok(())
}
