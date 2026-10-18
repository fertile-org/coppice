use crate::domain::context_profile::ContextProfile;
use crate::mcp::protocol::ToolDefinition;
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreTool {
    BoardList,
    BoardAgents,
    TicketGet,
    TicketComments,
    TicketRuns,
    TicketSearch,
    KnowledgeSearch,
    CommentPost,
    ResultSubmit,
    OpenRepo,
}

impl CoreTool {
    const ALL: [Self; 10] = [
        Self::BoardList,
        Self::BoardAgents,
        Self::TicketGet,
        Self::TicketComments,
        Self::TicketRuns,
        Self::TicketSearch,
        Self::KnowledgeSearch,
        Self::CommentPost,
        Self::ResultSubmit,
        Self::OpenRepo,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::BoardList => "board_list",
            Self::BoardAgents => "board_agents",
            Self::TicketGet => "ticket_get",
            Self::TicketComments => "ticket_comments",
            Self::TicketRuns => "ticket_runs",
            Self::TicketSearch => "ticket_search",
            Self::KnowledgeSearch => "knowledge_search",
            Self::CommentPost => "comment_post",
            Self::ResultSubmit => "result_submit",
            Self::OpenRepo => "open_repo",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.name() == name)
    }

    pub fn definition(self) -> ToolDefinition {
        let (description, input_schema, read_only) = match self {
            Self::BoardList => (
                "List every board by id and name.",
                object(json!({}), &[]),
                true,
            ),
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
                concat!(
                    "Search approved project knowledge relevant to this work. ",
                    crate::services::context_budget::knowledge_data_note!()
                ),
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
                object(result_submit_properties(), &["status", "summary"]),
                false,
            ),
            Self::OpenRepo => (
                "Check out a linked repository into this chat folder. Pass the slug from REPOS.md, not a path. Reuses a copy that is already here.",
                object(
                    json!({
                        "slug": { "type": "string", "description": "Repository slug from REPOS.md." }
                    }),
                    &["slug"],
                ),
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

    /// Conversation chat searches every board. Other profiles keep the
    /// board-scoped description.
    pub fn definition_for(self, scope: &crate::mcp::token::RunToolScope) -> ToolDefinition {
        let mut def = self.definition();
        if scope.workspace_chat() {
            match self {
                Self::TicketSearch => {
                    def.description = "Search tickets on every board by text and optional status. Each result includes the board name.".into();
                }
                Self::KnowledgeSearch => {
                    def.description = format!(
                        "Search approved knowledge on every board. Each result includes the board name. {}",
                        crate::services::context_budget::KNOWLEDGE_DATA_NOTE
                    );
                }
                _ => {}
            }
        }
        def
    }
}

/// Shape of the run result. Deliberately permissive: the server-side
/// deserializer and profile rules are the real validator, and report errors.
fn result_submit_properties() -> Value {
    let strings = |description: &str| {
        json!({ "type": "array", "items": { "type": "string" }, "description": description })
    };
    json!({
        "status": {
            "type": "string",
            "enum": ["done", "blocked", "continued"],
            "description": "done = work finished; blocked = cannot proceed (needs blockerType and mentionAgents); continued = partial progress, more to do (tickets only).",
        },
        "summary": { "type": "string", "description": "Markdown summary of what happened." },
        "changedFiles": strings("Paths changed (done/continued)."),
        "testsRun": strings("Tests or checks run (done/continued)."),
        "blockers": strings("Open blockers or caveats (done/continued)."),
        "nextStatus": { "type": "string", "description": "Suggested next ticket status (advisory)." },
        "assignTo": { "type": "string", "description": "Agent key to recommend for the next run (tickets only; see board_agents)." },
        "updatedDescription": { "type": "string", "description": "Replacement ticket description (tickets only)." },
        "acceptanceCriteria": { "type": "string", "description": "Acceptance criteria markdown (tickets only)." },
        "mentionAgents": strings("Agent keys to ask for input. At most 2 per run. Required (may be empty) for blocked."),
        "agentRequests": {
            "type": "array",
            "description": "Structured asks to other agents (done only).",
            "items": {
                "type": "object",
                "properties": {
                    "agentKey": { "type": "string" },
                    "intent": { "type": "string" },
                    "request": { "type": "string" },
                },
            },
        },
        "splitTickets": {
            "type": "array",
            "description": "Follow-up tickets to propose (done, tickets only).",
            "items": {
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "acceptanceCriteria": { "type": "string" },
                    "assignTo": { "type": "string" },
                },
                "required": ["title", "description"],
            },
        },
        "knowledgeCandidates": {
            "type": "array",
            "description": "Proposed knowledge items (knowledge compaction, done).",
            "items": { "type": "object", "additionalProperties": true },
        },
        "blockerType": {
            "type": "string",
            "description": "Required for blocked: missing_capability, missing_secret, permission or error.",
        },
        "requiredCapabilities": strings("Capabilities needed (blocked)."),
        "requiredSecrets": strings("Secret keys needed (blocked)."),
        "progressNote": { "type": "string", "description": "What remains to do (continued)." },
        "howItWorks": {
            "type": "string",
            "description": "Optional. Short note for Human Review: how the change works. Omit to report no brief.",
        },
        "changesFromPlan": {
            "type": "string",
            "description": "Optional. Short note for Human Review: what changed from the approved plan. Omit or leave empty when nothing changed.",
        },
        "lookHereFirst": {
            "type": "string",
            "description": "Optional. Short note for Human Review: where to look first. Omit to report no brief.",
        },
        "criteriaResults": {
            "type": "array",
            "description": "Optional (done). One entry per acceptance criterion in the approved plan: whether this change meets it and one line of evidence (test name, command output, or file).",
            "items": {
                "type": "object",
                "properties": {
                    "criterion": { "type": "string", "description": "The criterion text from the plan." },
                    "status": { "type": "string", "enum": ["met", "not_met", "not_checked"] },
                    "evidence": { "type": "string" },
                },
                "required": ["criterion", "status"],
            },
        },
        "planQuestions": {
            "type": "array",
            "description": "Planning runs only. 1 to 3 short questions to ask before writing the plan, when the answers would change it. Leave summary empty when you ask.",
            "items": {
                "type": "object",
                "properties": {
                    "question": { "type": "string" },
                    "choices": strings("Optional short answers to pick from."),
                },
                "required": ["question"],
            },
        },
    })
}

fn ticket_id_prop() -> Value {
    json!({ "type": "string", "description": "Ticket id; defaults to the current ticket." })
}

pub(crate) fn object(properties: Value, required: &[&str]) -> Value {
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
        ],
        ContextProfile::HumanChat => vec![
            BoardAgents,
            TicketGet,
            TicketComments,
            TicketRuns,
            TicketSearch,
            KnowledgeSearch,
            ResultSubmit,
        ],
        ContextProfile::Conversation => vec![
            BoardAgents,
            TicketGet,
            TicketComments,
            TicketRuns,
            TicketSearch,
            KnowledgeSearch,
            OpenRepo,
            ResultSubmit,
        ],
        ContextProfile::KnowledgeCompaction => vec![
            TicketGet,
            TicketComments,
            TicketRuns,
            KnowledgeSearch,
            ResultSubmit,
        ],
        ContextProfile::ConnectorCheck => vec![TicketGet, ResultSubmit],
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
        ];
        assert_eq!(names(ContextProfile::Full), all);
        assert_eq!(names(ContextProfile::HumanAgent), all);

        let no_comment: Vec<&str> = all
            .iter()
            .copied()
            .filter(|n| *n != "comment_post")
            .collect();
        assert_eq!(names(ContextProfile::HumanChat), no_comment);
        let mut conversation = no_comment.clone();
        conversation.insert(conversation.len() - 1, "open_repo");
        assert_eq!(names(ContextProfile::Conversation), conversation);
        assert!(CoreTool::OpenRepo.definition().read_only);

        assert_eq!(
            names(ContextProfile::KnowledgeCompaction),
            vec![
                "ticket_get",
                "ticket_comments",
                "ticket_runs",
                "knowledge_search",
                "result_submit",
            ]
        );
    }

    #[test]
    fn connector_check_profile_has_two_tools() {
        assert_eq!(
            names(ContextProfile::ConnectorCheck),
            vec!["ticket_get", "result_submit"]
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

    #[test]
    fn result_submit_brief_fields_are_optional() {
        let schema = CoreTool::ResultSubmit.definition().input_schema;
        let required = schema["required"].as_array().expect("required");
        for name in ["howItWorks", "changesFromPlan", "lookHereFirst"] {
            assert!(schema["properties"][name].is_object(), "{name}");
            assert!(!required.iter().any(|item| item == name), "{name}");
        }
    }

    #[test]
    fn from_name_roundtrips() {
        for tool in CoreTool::ALL {
            assert_eq!(CoreTool::from_name(tool.name()), Some(tool));
        }
        assert_eq!(CoreTool::from_name("skill_list"), None);
    }
}
