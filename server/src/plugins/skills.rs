use crate::domain::context_profile::ContextProfile;
use crate::domain::substatus::TicketStatus;
use crate::domain::ticket::status_to_str;
use crate::domain::workflow::{
    is_in_qa_qc_task, is_in_review_review_task, is_pm_identity, is_ready_tech_lead_refinement,
};
use crate::plugins::builtin::BUILTIN_PLUGIN;
use anyhow::Context;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillInfo {
    pub id: String,
    pub description: String,
    /// Absolute path of the skill folder (contains `SKILL.md`).
    pub path: PathBuf,
}

#[derive(Debug, Default)]
pub struct SkillCatalog {
    builtin: Vec<(SkillInfo, String)>,
}

pub(crate) struct Frontmatter {
    pub(crate) name: String,
    pub(crate) description: String,
}

/// Splits `---\n<yaml>\n---\n<body>`; reads the flat `name` / `description` keys.
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

    let (mut name, mut description) = (None, None);
    for line in yaml.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value
            .trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .to_string();
        match key.trim() {
            "name" => name = Some(value),
            "description" => description = Some(value),
            _ => {}
        }
    }
    let name = name
        .filter(|n| !n.is_empty())
        .context("frontmatter `name` is required")?;
    let description = description
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
        Ok(SkillCatalog { builtin })
    }

    pub fn skills_for(&self, _agent_id: Uuid) -> Vec<SkillInfo> {
        self.builtin.iter().map(|(i, _)| i.clone()).collect()
    }

    pub fn get(&self, _agent_id: Uuid, id: &str) -> Option<(SkillInfo, String)> {
        self.builtin.iter().find(|(i, _)| i.id == id).cloned()
    }
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

    fn catalog() -> (tempfile::TempDir, SkillCatalog) {
        let dir = tempfile::tempdir().unwrap();
        materialize_builtin(dir.path()).unwrap();
        let catalog = load_builtin(dir.path()).unwrap();
        (dir, catalog)
    }

    #[test]
    fn builtin_skills_parse_frontmatter() {
        let (_dir, catalog) = catalog();
        let skills = catalog.skills_for(Uuid::new_v4());
        assert_eq!(skills.len(), 6);
        for (id, _) in BUILTIN_SKILLS {
            let info = skills
                .iter()
                .find(|s| s.id == *id)
                .unwrap_or_else(|| panic!("missing skill {id}"));
            assert!(!info.description.trim().is_empty(), "{id} description");
            assert!(info.path.is_absolute(), "{id} path");
            assert!(info.path.join("SKILL.md").is_file(), "{id} SKILL.md");
            let (_, body) = catalog.get(Uuid::new_v4(), id).unwrap();
            assert!(!body.trim().is_empty(), "{id} body");
            assert!(!body.starts_with("---"), "{id} body includes frontmatter");
        }
        assert!(catalog.get(Uuid::new_v4(), "nope").is_none());
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
        assert_eq!(
            load_builtin(dir.path())
                .unwrap()
                .skills_for(Uuid::new_v4())
                .len(),
            6
        );
    }

    #[test]
    fn load_builtin_skips_malformed_skill() {
        let dir = tempfile::tempdir().unwrap();
        materialize_builtin(dir.path()).unwrap();
        let bad = dir.path().join("coppice/skills/broken");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join("SKILL.md"), "no frontmatter here").unwrap();
        let catalog = load_builtin(dir.path()).unwrap();
        assert_eq!(catalog.skills_for(Uuid::new_v4()).len(), 6);
        assert!(catalog.get(Uuid::new_v4(), "broken").is_none());
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
