use crate::domain::context_profile::ContextProfile;
use crate::domain::substatus::TicketStatus;
use crate::domain::ticket::status_to_str;
use crate::domain::workflow::{
    is_in_qa_qc_task, is_in_review_review_task, is_pm_identity, is_ready_tech_lead_refinement,
};
use crate::plugins::builtin::BUILTIN_PLUGIN;
use crate::plugins::manifest::PluginManifest;
use anyhow::Context;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillInfo {
    pub id: String,
    pub description: String,
    /// Absolute path of the skill folder (contains `SKILL.md`).
    pub path: PathBuf,
}

/// Served skills of one enabled plugin. `SkillInfo.id` is `<plugin>:<skill>`.
#[derive(Debug, Clone)]
pub struct PluginSkillSet {
    pub plugin_id: Uuid,
    pub plugin_name: String,
    /// Canonical plugin root; skill bodies must resolve inside it.
    pub root: PathBuf,
    pub skills: Vec<SkillInfo>,
}

impl PluginSkillSet {
    /// Valid skills only; for duplicate names the first by `rel_path` wins.
    pub fn from_manifest(plugin_id: Uuid, root: &Path, manifest: &PluginManifest) -> Self {
        let mut entries: Vec<_> = manifest
            .skills
            .iter()
            .filter(|s| s.error.is_none())
            .collect();
        entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
        let mut seen = HashSet::new();
        let mut skills: Vec<SkillInfo> = entries
            .into_iter()
            .filter(|s| seen.insert(s.name.as_str()))
            .map(|s| SkillInfo {
                id: format!("{}:{}", manifest.name, s.name),
                description: s.description.clone(),
                path: root.join(&s.rel_path),
            })
            .collect();
        skills.sort_by(|a, b| a.id.cmp(&b.id));
        PluginSkillSet {
            plugin_id,
            plugin_name: manifest.name.clone(),
            root: root.to_path_buf(),
            skills,
        }
    }
}

#[derive(Debug, Default)]
pub struct SkillCatalog {
    builtin: Vec<(SkillInfo, String)>,
    plugins: RwLock<HashMap<Uuid, PluginSkillSet>>,
    refresh: tokio::sync::Mutex<()>,
}

#[derive(Debug)]
pub(crate) struct Frontmatter {
    pub(crate) name: String,
    pub(crate) description: String,
}

/// Splits `---\n<yaml>\n---\n<body>`; reads the string `name` / `description`
/// keys, collapsing description whitespace to single spaces.
pub(crate) fn parse_skill_file(text: &str) -> anyhow::Result<(Frontmatter, String)> {
    let text = text.replace("\r\n", "\n");
    let rest = text
        .strip_prefix("---\n")
        .context("SKILL.md must start with YAML frontmatter")?;
    let end = rest
        .find("\n---")
        .context("SKILL.md frontmatter is not closed")?;
    let (yaml, after) = rest.split_at(end);
    let body = after["\n---".len()..].trim_start_matches('\n').to_string();

    let docs = yaml_rust2::YamlLoader::load_from_str(yaml)
        .map_err(|err| anyhow::anyhow!("invalid frontmatter YAML: {err}"))?;
    let doc = docs.first();
    let field = |key: &str| doc.and_then(|d| d[key].as_str());
    let name = field("name")
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .context("frontmatter `name` is required")?;
    let description = field("description")
        .map(|d| d.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|d| !d.is_empty())
        .context("frontmatter `description` is required")?;
    Ok((Frontmatter { name, description }, body))
}

impl SkillCatalog {
    /// Loads `<dir>/coppice/skills/*/SKILL.md`. Skill ids are directory names.
    pub fn load_builtin(dir: &Path) -> anyhow::Result<SkillCatalog> {
        let skills_dir = dir.join(BUILTIN_PLUGIN).join("skills");
        let skills_dir = std::fs::canonicalize(&skills_dir)
            .with_context(|| format!("built-in skills dir {}", skills_dir.display()))?;
        let mut builtin = Vec::new();
        for entry in std::fs::read_dir(&skills_dir)? {
            let path = entry?.path();
            match load_skill_dir(&path) {
                Ok(Some(skill)) => builtin.push(skill),
                Ok(None) => {}
                Err(err) => {
                    tracing::warn!(path = %path.display(), error = %format!("{err:#}"), "skipping unreadable built-in skill");
                }
            }
        }
        builtin.sort_by(|a, b| a.0.id.cmp(&b.0.id));
        Ok(SkillCatalog {
            builtin,
            plugins: RwLock::default(),
            refresh: tokio::sync::Mutex::default(),
        })
    }

    /// Held across a refresh's DB read and swap so the last refresh to take it
    /// installs the newest state.
    pub async fn lock_refresh(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.refresh.lock().await
    }

    /// Stops serving one plugin's skills; waits for any in-flight refresh so a
    /// stale snapshot cannot re-add it afterwards.
    pub async fn remove_plugin(&self, plugin_id: Uuid) {
        let _refresh = self.lock_refresh().await;
        self.plugins
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&plugin_id);
    }

    pub fn set_plugin_skills(&self, sets: Vec<PluginSkillSet>) {
        let sets = sets.into_iter().map(|s| (s.plugin_id, s)).collect();
        *self.plugins.write().unwrap_or_else(|e| e.into_inner()) = sets;
    }

    /// Built-ins first, then skills of the snapshot's plugins sorted by id.
    pub fn skills_for(&self, plugin_ids: &[Uuid]) -> Vec<SkillInfo> {
        let mut skills: Vec<SkillInfo> = self.builtin.iter().map(|(i, _)| i.clone()).collect();
        let mut from_plugins: Vec<SkillInfo> = {
            let plugins = self.plugins.read().unwrap_or_else(|e| e.into_inner());
            plugin_ids
                .iter()
                .filter_map(|id| plugins.get(id))
                .flat_map(|set| set.skills.iter().cloned())
                .collect()
        };
        from_plugins.sort_by(|a, b| a.id.cmp(&b.id));
        skills.extend(from_plugins);
        skills
    }

    /// Plugin skill bodies are read from disk on every call.
    pub fn get(&self, plugin_ids: &[Uuid], id: &str) -> Option<(SkillInfo, String)> {
        let builtin_id = id
            .strip_prefix(BUILTIN_PLUGIN)
            .and_then(|rest| rest.strip_prefix(':'))
            .unwrap_or(id);
        if let Some(found) = self.builtin.iter().find(|(i, _)| i.id == builtin_id) {
            return Some(found.clone());
        }
        let (info, root) = {
            let plugins = self.plugins.read().unwrap_or_else(|e| e.into_inner());
            plugin_ids
                .iter()
                .filter_map(|pid| plugins.get(pid))
                .find_map(|set| {
                    set.skills
                        .iter()
                        .find(|s| s.id == id)
                        .map(|s| (s.clone(), set.root.clone()))
                })?
        };
        match read_plugin_skill_body(&root, &info.path) {
            Ok(body) => Some((info, body)),
            Err(err) => {
                tracing::warn!(skill = %info.id, error = %format!("{err:#}"), "plugin skill unavailable");
                None
            }
        }
    }
}

fn read_plugin_skill_body(root: &Path, skill_dir: &Path) -> anyhow::Result<String> {
    let root = std::fs::canonicalize(root)?;
    let skill_file = std::fs::canonicalize(skill_dir.join("SKILL.md"))?;
    anyhow::ensure!(skill_file.starts_with(&root), "path escapes plugin root");
    let text = std::fs::read_to_string(&skill_file)?;
    Ok(parse_skill_file(&text)?.1)
}

pub fn load_builtin(dir: &Path) -> anyhow::Result<SkillCatalog> {
    SkillCatalog::load_builtin(dir)
}

fn load_skill_dir(path: &Path) -> anyhow::Result<Option<(SkillInfo, String)>> {
    let skill_file = path.join("SKILL.md");
    if !skill_file.is_file() {
        return Ok(None);
    }
    let id = path
        .file_name()
        .and_then(|n| n.to_str())
        .with_context(|| format!("skill dir name {}", path.display()))?
        .to_string();
    let text = std::fs::read_to_string(&skill_file)
        .with_context(|| format!("read {}", skill_file.display()))?;
    let (frontmatter, body) =
        parse_skill_file(&text).with_context(|| format!("parse {}", skill_file.display()))?;
    if frontmatter.name != id {
        tracing::warn!(skill = %id, name = %frontmatter.name, "skill frontmatter name differs from directory; using directory name");
    }
    Ok(Some((
        SkillInfo {
            id,
            description: frontmatter.description,
            path: path.to_path_buf(),
        },
        body,
    )))
}

/// The skill an agent must load before starting this kind of run, if any.
pub fn required_skill(
    profile: ContextProfile,
    job_type: &str,
    ticket_status: Option<TicketStatus>,
    agent_key: &str,
    agent_role: &str,
) -> Option<&'static str> {
    if !matches!(profile, ContextProfile::Full | ContextProfile::HumanAgent)
        || job_type != "work_on_ticket"
    {
        return None;
    }
    if profile == ContextProfile::HumanAgent {
        return Some("coppice-git");
    }
    let status = ticket_status.map(status_to_str).unwrap_or_default();
    // Mirrors the precedence of the legacy contract guidance: the stable Tech
    // Lead identity on a Ready ticket wins over a PM-like editable role.
    let ready_tech_lead =
        is_ready_tech_lead_refinement(profile, job_type, status, agent_key, agent_role);
    if is_pm_identity(agent_key, agent_role) && !ready_tech_lead {
        Some("coppice-pm-refinement")
    } else if ready_tech_lead || is_in_review_review_task(status, agent_key, agent_role) {
        Some("coppice-tech-lead-review")
    } else if is_in_qa_qc_task(status, agent_key, agent_role) {
        Some("coppice-qc-verification")
    } else {
        Some("coppice-git")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::builtin::{materialize_builtin, BUILTIN_SKILLS};
    use crate::plugins::manifest::SkillEntry;

    fn catalog() -> (tempfile::TempDir, SkillCatalog) {
        let dir = tempfile::tempdir().unwrap();
        materialize_builtin(dir.path()).unwrap();
        let catalog = load_builtin(dir.path()).unwrap();
        (dir, catalog)
    }

    #[test]
    fn builtin_skills_parse_frontmatter() {
        let (_dir, catalog) = catalog();
        let skills = catalog.skills_for(&[]);
        assert_eq!(skills.len(), 6);
        for (id, _) in BUILTIN_SKILLS {
            let info = skills
                .iter()
                .find(|s| s.id == *id)
                .unwrap_or_else(|| panic!("missing skill {id}"));
            assert!(!info.description.trim().is_empty(), "{id} description");
            assert!(info.path.is_absolute(), "{id} path");
            assert!(info.path.join("SKILL.md").is_file(), "{id} SKILL.md");
            let (_, body) = catalog.get(&[], id).unwrap();
            assert!(!body.trim().is_empty(), "{id} body");
            assert!(!body.starts_with("---"), "{id} body includes frontmatter");
        }
        assert!(catalog.get(&[], "nope").is_none());
    }

    fn sample_plugin_set(dir: &tempfile::TempDir) -> PluginSkillSet {
        let root = dir.path().join("sample-plugin");
        copy_tree(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/plugins/sample-plugin"),
            &root,
        );
        let root = std::fs::canonicalize(root).unwrap();
        let manifest = crate::plugins::manifest::parse_plugin(&root).unwrap();
        PluginSkillSet::from_manifest(Uuid::new_v4(), &root, &manifest)
    }

    fn copy_tree(src: &Path, dst: &Path) {
        std::fs::create_dir_all(dst).unwrap();
        for entry in std::fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let target = dst.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    #[test]
    fn plugin_skills_are_namespaced_and_scoped_to_snapshot() {
        let (_dir, catalog) = catalog();
        let plugins = tempfile::tempdir().unwrap();
        let set = sample_plugin_set(&plugins);
        let id = set.plugin_id;
        catalog.set_plugin_skills(vec![set]);

        let ids: Vec<String> = catalog
            .skills_for(&[id])
            .into_iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(ids.len(), 7, "{ids:?}");
        assert_eq!(ids.last().map(String::as_str), Some("sample-plugin:hello"));
        assert!(!ids.iter().any(|i| i.contains("broken")), "{ids:?}");

        let builtins = catalog.skills_for(&[]);
        assert_eq!(builtins.len(), 6);
        assert!(builtins.iter().all(|s| !s.id.contains(':')));
    }

    #[test]
    fn builtin_resolves_with_and_without_coppice_prefix() {
        let (_dir, catalog) = catalog();
        let (plain, body) = catalog.get(&[], "coppice-git").unwrap();
        let (prefixed, prefixed_body) = catalog.get(&[], "coppice:coppice-git").unwrap();
        assert_eq!(plain, prefixed);
        assert_eq!(body, prefixed_body);
    }

    #[test]
    fn plugin_skill_body_reflects_disk_changes() {
        let (_dir, catalog) = catalog();
        let plugins = tempfile::tempdir().unwrap();
        let set = sample_plugin_set(&plugins);
        let id = set.plugin_id;
        let skill_file = set.root.join("skills/hello/SKILL.md");
        catalog.set_plugin_skills(vec![set]);

        let (info, body) = catalog.get(&[id], "sample-plugin:hello").unwrap();
        assert_eq!(info.id, "sample-plugin:hello");
        assert_eq!(info.description, "Says hello");
        assert_eq!(body.trim(), "Say hello to the user.");

        std::fs::write(
            &skill_file,
            "---\nname: hello\ndescription: Says hello\n---\nWave instead.\n",
        )
        .unwrap();
        let (_, body) = catalog.get(&[id], "sample-plugin:hello").unwrap();
        assert_eq!(body.trim(), "Wave instead.");
    }

    #[test]
    fn plugin_skill_not_in_snapshot_is_none() {
        let (_dir, catalog) = catalog();
        let plugins = tempfile::tempdir().unwrap();
        let set = sample_plugin_set(&plugins);
        catalog.set_plugin_skills(vec![set]);
        assert!(catalog.get(&[], "sample-plugin:hello").is_none());
        assert!(catalog
            .get(&[Uuid::new_v4()], "sample-plugin:hello")
            .is_none());
    }

    #[tokio::test]
    async fn remove_plugin_stops_serving_its_skills() {
        let (_dir, catalog) = catalog();
        let plugins = tempfile::tempdir().unwrap();
        let set = sample_plugin_set(&plugins);
        let id = set.plugin_id;
        catalog.set_plugin_skills(vec![set]);
        assert!(catalog.get(&[id], "sample-plugin:hello").is_some());

        catalog.remove_plugin(id).await;
        assert_eq!(catalog.skills_for(&[id]).len(), 6);
        assert!(catalog.get(&[id], "sample-plugin:hello").is_none());
    }

    #[test]
    fn plugin_skill_escaping_root_is_none() {
        let (_dir, catalog) = catalog();
        let plugins = tempfile::tempdir().unwrap();
        let set = sample_plugin_set(&plugins);
        let id = set.plugin_id;
        let skill_dir = set.root.join("skills/hello");
        catalog.set_plugin_skills(vec![set]);

        let outside = tempfile::tempdir().unwrap();
        std::fs::write(
            outside.path().join("SKILL.md"),
            "---\nname: hello\ndescription: x\n---\nsecret",
        )
        .unwrap();
        std::fs::remove_dir_all(&skill_dir).unwrap();
        std::os::unix::fs::symlink(outside.path(), &skill_dir).unwrap();
        assert!(catalog.get(&[id], "sample-plugin:hello").is_none());
    }

    #[test]
    fn duplicate_skill_names_keep_first_by_rel_path() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let manifest = crate::plugins::manifest::PluginManifest {
            skills: vec![
                SkillEntry {
                    name: "dup".into(),
                    description: "second".into(),
                    rel_path: "z/dup".into(),
                    error: None,
                },
                SkillEntry {
                    name: "dup".into(),
                    description: "first".into(),
                    rel_path: "a/dup".into(),
                    error: None,
                },
            ],
            ..sample_manifest()
        };
        let set = PluginSkillSet::from_manifest(Uuid::new_v4(), &root, &manifest);
        assert_eq!(set.skills.len(), 1);
        assert_eq!(set.skills[0].id, "p:dup");
        assert_eq!(set.skills[0].description, "first");
        assert_eq!(set.skills[0].path, root.join("a/dup"));
    }

    fn sample_manifest() -> crate::plugins::manifest::PluginManifest {
        crate::plugins::manifest::PluginManifest {
            name: "p".into(),
            version: "0.0.0".into(),
            description: String::new(),
            author: None,
            layout: crate::plugins::manifest::PluginLayout::SkillsOnly,
            skills: Vec::new(),
            mcp_servers: Vec::new(),
            unsupported: Vec::new(),
            marketplace: None,
            external: None,
        }
    }

    #[test]
    fn materialize_overwrites_stale_files() {
        let dir = tempfile::tempdir().unwrap();
        materialize_builtin(dir.path()).unwrap();
        let file = dir.path().join("coppice/skills/coppice-git/SKILL.md");
        std::fs::write(&file, "stale").unwrap();
        materialize_builtin(dir.path()).unwrap();
        assert_eq!(std::fs::read_to_string(file).unwrap(), BUILTIN_SKILLS[5].1);
    }

    #[test]
    fn materialize_prunes_removed_skills_only_under_skills_dir() {
        let dir = tempfile::tempdir().unwrap();
        materialize_builtin(dir.path()).unwrap();
        let stale = dir.path().join("coppice/skills/coppice-retired");
        std::fs::create_dir_all(&stale).unwrap();
        std::fs::write(
            stale.join("SKILL.md"),
            "---\nname: x\ndescription: y\n---\nold",
        )
        .unwrap();
        let sibling = dir.path().join("coppice/plugin.toml");
        std::fs::write(&sibling, "keep").unwrap();
        materialize_builtin(dir.path()).unwrap();
        assert!(!stale.exists());
        assert!(sibling.is_file());
        assert_eq!(load_builtin(dir.path()).unwrap().skills_for(&[]).len(), 6);
    }

    #[test]
    fn load_builtin_skips_malformed_skill() {
        let dir = tempfile::tempdir().unwrap();
        materialize_builtin(dir.path()).unwrap();
        let bad = dir.path().join("coppice/skills/broken");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join("SKILL.md"), "no frontmatter here").unwrap();
        let catalog = load_builtin(dir.path()).unwrap();
        assert_eq!(catalog.skills_for(&[]).len(), 6);
        assert!(catalog.get(&[], "broken").is_none());
    }

    #[test]
    fn frontmatter_folded_description_is_one_line() {
        let (fm, body) = parse_skill_file(
            "---\nname: pdf\ndescription: >\n  Fill PDF forms.\n  Use for PDFs.\n---\nBody\n",
        )
        .unwrap();
        assert_eq!(fm.name, "pdf");
        assert_eq!(fm.description, "Fill PDF forms. Use for PDFs.");
        assert_eq!(body, "Body\n");
    }

    #[test]
    fn frontmatter_literal_description_collapses() {
        let (fm, _) =
            parse_skill_file("---\nname: a\ndescription: |\n  line one\n  line two\n---\n")
                .unwrap();
        assert_eq!(fm.description, "line one line two");
    }

    #[test]
    fn frontmatter_quoted_and_extra_keys() {
        let (fm, _) = parse_skill_file(
            "---\nname: \"a-b\"\ndescription: 'Says: hi'\nlicense: MIT\nmetadata:\n  x: 1\n---\n",
        )
        .unwrap();
        assert_eq!(
            (fm.name.as_str(), fm.description.as_str()),
            ("a-b", "Says: hi")
        );
    }

    #[test]
    fn frontmatter_crlf_ok() {
        assert!(parse_skill_file("---\r\nname: a\r\ndescription: d\r\n---\r\nB").is_ok());
    }

    #[test]
    fn frontmatter_errors() {
        let err = |t: &str| format!("{:#}", parse_skill_file(t).unwrap_err());
        assert!(err("---\ndescription: d\n---\n").contains("frontmatter `name` is required"));
        assert!(err("---\nname: a\n---\n").contains("frontmatter `description` is required"));
        assert!(err("---\nname: [1, 2]\ndescription: d\n---\n")
            .contains("frontmatter `name` is required"));
        assert!(err("---\nname: a\ndescription: d\n  bad: [\n---\n")
            .contains("invalid frontmatter YAML"));
        assert!(err("no frontmatter").contains("SKILL.md must start with YAML frontmatter"));
    }

    #[test]
    fn required_skill_for_qc_in_qa_is_qc_verification() {
        assert_eq!(
            required_skill(
                ContextProfile::Full,
                "work_on_ticket",
                Some(TicketStatus::InQa),
                "qc",
                "Quality Control",
            ),
            Some("coppice-qc-verification")
        );
    }

    #[test]
    fn required_skill_for_pm_and_tech_lead() {
        let pm = required_skill(
            ContextProfile::Full,
            "work_on_ticket",
            Some(TicketStatus::Backlog),
            "pm",
            "Product Manager",
        );
        assert_eq!(pm, Some("coppice-pm-refinement"));
        let ready = required_skill(
            ContextProfile::Full,
            "work_on_ticket",
            Some(TicketStatus::Ready),
            "tech_lead",
            "Technical Lead",
        );
        assert_eq!(ready, Some("coppice-tech-lead-review"));
        let review = required_skill(
            ContextProfile::Full,
            "work_on_ticket",
            Some(TicketStatus::InReview),
            "reviewer",
            "Reviewer",
        );
        assert_eq!(review, Some("coppice-tech-lead-review"));
        // A PM-like role on a Ready ticket held by the Tech Lead identity is
        // still a Tech Lead run.
        let lead_pm_role = required_skill(
            ContextProfile::Full,
            "work_on_ticket",
            Some(TicketStatus::Ready),
            "tech_lead",
            "Product Manager",
        );
        assert_eq!(lead_pm_role, Some("coppice-tech-lead-review"));
    }

    #[test]
    fn required_skill_for_implementer_is_git() {
        assert_eq!(
            required_skill(
                ContextProfile::Full,
                "work_on_ticket",
                Some(TicketStatus::InProgress),
                "backend_engineer",
                "Backend Engineer",
            ),
            Some("coppice-git")
        );
    }

    #[test]
    fn required_skill_none_for_chat() {
        for profile in [
            ContextProfile::HumanChat,
            ContextProfile::Conversation,
            ContextProfile::KnowledgeCompaction,
        ] {
            assert_eq!(
                required_skill(profile, "chat_turn", None, "qc", "Quality Control"),
                None
            );
        }
        assert_eq!(
            required_skill(
                ContextProfile::Full,
                "respond_to_mention",
                Some(TicketStatus::InQa),
                "qc",
                "Quality Control",
            ),
            None
        );
    }
}
