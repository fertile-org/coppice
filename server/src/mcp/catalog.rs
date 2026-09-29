use crate::domain::context_profile::ContextProfile;
use crate::mcp::protocol::ToolDefinition;
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreTool {
    BoardAgents,
    TicketGet,
    TicketComments,
    TicketRuns,
    TicketSearch,
    KnowledgeSearch,
    CommentPost,
    ResultSubmit,
    SkillList,
    SkillLoad,
}

impl CoreTool {
    pub fn name(self) -> &'static str {
        match self {
            Self::BoardAgents => "board_agents",
            Self::TicketGet => "ticket_get",
            Self::TicketComments => "ticket_comments",
            Self::TicketRuns => "ticket_runs",
            Self::TicketSearch => "ticket_search",
            Self::KnowledgeSearch => "knowledge_search",
            Self::CommentPost => "comment_post",
            Self::ResultSubmit => "result_submit",
            Self::SkillList => "skill_list",
            Self::SkillLoad => "skill_load",
        }
    }

    pub fn is_skill(self) -> bool {
        matches!(self, Self::SkillList | Self::SkillLoad)
    }

    pub fn definition(self) -> ToolDefinition {
        let (description, input_schema, read_only) = match self {
            Self::BoardAgents => (
                "List enabled agents (key, name, role). Use a key to hand work to an agent.",
                object(json!({}), &[]),
                true,
            ),
            Self::TicketGet => (
                "Get a ticket: title, description, acceptance criteria, status, assignee, repository, branch. Defaults to the current ticket.",
                object(json!({ "ticketId": ticket_id_prop() }), &[]),
                true,
            ),
            Self::TicketComments => (
                "List ticket comments, newest first. Pass the returned nextBefore as `before` for older comments.",
                object(
                    json!({
                        "ticketId": ticket_id_prop(),
                        "limit": { "type": "integer", "minimum": 1, "maximum": 50, "description": "Default 20." },
                        "before": { "type": "string", "description": "Comment id to page from." },
                    }),
                    &[],
                ),
                true,
            ),
            Self::TicketRuns => (
                "List past agent run summaries for a ticket, newest first.",
                object(
                    json!({
                        "ticketId": ticket_id_prop(),
                        "limit": { "type": "integer", "minimum": 1, "maximum": 30, "description": "Default 10." },
                    }),
                    &[],
                ),
                true,
            ),
            Self::TicketSearch => (
                "Search tickets on the current board by text and optional status.",
                object(
                    json!({
                        "query": { "type": "string" },
                        "status": { "type": "string" },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 50, "description": "Default 20." },
                    }),
                    &[],
                ),
                true,
            ),
            Self::KnowledgeSearch => (
                "Search approved project knowledge relevant to this work.",
                object(
                    json!({
                        "query": { "type": "string" },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 20, "description": "Default 5." },
                    }),
                    &["query"],
                ),
                true,
            ),
            Self::CommentPost => (
                "Post a markdown note on the current ticket as this agent. Limited per run.",
                object(json!({ "body": { "type": "string" } }), &["body"]),
                false,
            ),
            Self::ResultSubmit => (
                "Submit the final result for this run. Returns validation errors and warnings; fix and resubmit if it reports errors.",
                object(
                    json!({
                        "status": { "type": "string", "enum": ["done", "blocked", "continued"] },
                    }),
                    &["status"],
                ),
                false,
            ),
            Self::SkillList => (
                "List skills available to this run (name, description).",
                object(json!({}), &[]),
                true,
            ),
            Self::SkillLoad => (
                "Load a skill's instructions and its absolute folder path.",
                object(json!({ "name": { "type": "string" } }), &["name"]),
                true,
            ),
        };
        ToolDefinition {
            name: self.name().to_string(),
            description: description.to_string(),
            input_schema,
            read_only,
        }
    }
}

fn ticket_id_prop() -> Value {
    json!({ "type": "string", "description": "Ticket id; defaults to the current ticket." })
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": true,
    })
}

pub fn core_tools_for(profile: ContextProfile) -> Vec<CoreTool> {
    use CoreTool::*;
    match profile {
        ContextProfile::Full | ContextProfile::HumanAgent => vec![
            BoardAgents,
            TicketGet,
            TicketComments,
            TicketRuns,
            TicketSearch,
            KnowledgeSearch,
            CommentPost,
            ResultSubmit,
            SkillList,
            SkillLoad,
        ],
        ContextProfile::HumanChat | ContextProfile::Conversation => vec![
            BoardAgents,
            TicketGet,
            TicketComments,
            TicketRuns,
            TicketSearch,
            KnowledgeSearch,
            ResultSubmit,
            SkillList,
            SkillLoad,
        ],
        ContextProfile::KnowledgeCompaction => vec![
            TicketGet,
            TicketComments,
            TicketRuns,
            KnowledgeSearch,
            SkillList,
            SkillLoad,
            ResultSubmit,
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(profile: ContextProfile) -> Vec<&'static str> {
        core_tools_for(profile)
            .into_iter()
            .map(CoreTool::name)
            .collect()
    }

    #[test]
    fn catalog_matrix_matches_spec() {
        let all = vec![
            "board_agents",
            "ticket_get",
            "ticket_comments",
            "ticket_runs",
            "ticket_search",
            "knowledge_search",
            "comment_post",
            "result_submit",
            "skill_list",
            "skill_load",
        ];
        assert_eq!(names(ContextProfile::Full), all);
        assert_eq!(names(ContextProfile::HumanAgent), all);

        let no_comment: Vec<&str> = all
            .iter()
            .copied()
            .filter(|n| *n != "comment_post")
            .collect();
        assert_eq!(names(ContextProfile::HumanChat), no_comment);
        assert_eq!(names(ContextProfile::Conversation), no_comment);

        assert_eq!(
            names(ContextProfile::KnowledgeCompaction),
            vec![
                "ticket_get",
                "ticket_comments",
                "ticket_runs",
                "knowledge_search",
                "skill_list",
                "skill_load",
                "result_submit",
            ]
        );
    }

    #[test]
    fn definitions_are_named_and_object_schemas() {
        for tool in core_tools_for(ContextProfile::Full) {
            let def = tool.definition();
            assert_eq!(def.name, tool.name());
            assert_eq!(def.input_schema["type"], "object");
            assert!(!def.description.is_empty());
        }
        assert!(!CoreTool::CommentPost.definition().read_only);
        assert!(CoreTool::TicketGet.definition().read_only);
    }
}
