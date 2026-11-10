//! `knowledgeCandidates` from a compaction run: lenient parsing, strict validation.
//! Invalid candidates are dropped with a reason; they never fail the batch.

use crate::domain::knowledge::{
    confidence_from_str, scope_from_str, type_from_str, validate_revision, KnowledgeConfidence,
    KnowledgeRevisionInput, KnowledgeScope, KnowledgeSourceType,
};
use crate::knowledge::citations::{validate_citation_path, MAX_CITATIONS_PER_CANDIDATE};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Ids stay strings so one malformed candidate cannot fail the whole result parse.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct KnowledgeCandidateSpec {
    #[serde(rename = "type")]
    pub knowledge_type: String,
    pub scope: String,
    pub board_id: Option<String>,
    pub agent_id: Option<String>,
    pub title: String,
    pub content: String,
    pub confidence: Option<String>,
    pub source_ticket_ids: Vec<String>,
    pub supersedes_item_id: Option<String>,
    pub requires_human_approval: bool,
    /// Repo-relative paths the candidate rests on.
    pub citations: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct BatchTicket {
    pub id: Uuid,
    pub board_id: Uuid,
}

/// Where an item that may be superseded applies.
#[derive(Debug, Clone, Copy)]
pub struct SupersedableItem {
    pub scope: KnowledgeScope,
    /// `None` for workspace scope.
    pub board_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
}

pub struct CandidateContext<'a> {
    pub tickets: &'a [BatchTicket],
    pub agent_ids: &'a HashSet<Uuid>,
    /// Approved, live items that may be superseded, keyed by id.
    pub supersedable: &'a HashMap<Uuid, SupersedableItem>,
    pub run_id: Uuid,
    pub max_candidates: usize,
}

#[derive(Debug, Clone)]
pub struct AcceptedCandidate {
    /// Position in the agent's list; stable key for idempotent inserts.
    pub index: i32,
    pub input: KnowledgeRevisionInput,
    pub source_ticket_ids: Vec<Uuid>,
    pub supersedes_item_id: Option<Uuid>,
    pub requires_human_approval: bool,
    /// Validated, de-duplicated, capped.
    pub citations: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CandidateReview {
    pub accepted: Vec<AcceptedCandidate>,
    pub dropped: Vec<String>,
}

pub fn review_candidates(
    specs: &[KnowledgeCandidateSpec],
    context: &CandidateContext<'_>,
) -> CandidateReview {
    let mut review = CandidateReview::default();
    for (position, spec) in specs.iter().enumerate() {
        if review.accepted.len() >= context.max_candidates {
            review.dropped.push(format!(
                "candidate {}: over the limit of {} candidates per batch",
                position + 1,
                context.max_candidates
            ));
            continue;
        }
        match accept(position, spec, context) {
            Ok((candidate, citation_notes)) => {
                review.dropped.extend(citation_notes);
                review.accepted.push(candidate);
            }
            Err(reason) => review
                .dropped
                .push(format!("candidate {}: {reason}", position + 1)),
        }
    }
    review
}

fn accept(
    position: usize,
    spec: &KnowledgeCandidateSpec,
    context: &CandidateContext<'_>,
) -> Result<(AcceptedCandidate, Vec<String>), String> {
    let knowledge_type = type_from_str(spec.knowledge_type.trim())
        .ok_or_else(|| format!("unknown type `{}`", spec.knowledge_type))?;
    let scope = scope_from_str(spec.scope.trim())
        .ok_or_else(|| format!("unknown scope `{}`", spec.scope))?;
    let confidence = match spec.confidence.as_deref().map(str::trim) {
        None | Some("") => KnowledgeConfidence::Medium,
        Some(value) => {
            confidence_from_str(value).ok_or_else(|| format!("unknown confidence `{value}`"))?
        }
    };

    let mut source_ticket_ids = Vec::new();
    for raw in &spec.source_ticket_ids {
        let ticket_id = parse_id(raw, "sourceTicketIds")?;
        if !context.tickets.iter().any(|ticket| ticket.id == ticket_id) {
            return Err(format!("source ticket {ticket_id} is not in this batch"));
        }
        if !source_ticket_ids.contains(&ticket_id) {
            source_ticket_ids.push(ticket_id);
        }
    }
    // A supersession target is looked up first: with no source tickets the
    // candidate is a merge and takes its board from that item.
    let supersedes = match spec.supersedes_item_id.as_deref() {
        None => None,
        Some(raw) if raw.trim().is_empty() => None,
        Some(raw) => {
            let item_id = parse_id(raw, "supersedesItemId")?;
            let item = *context.supersedable.get(&item_id).ok_or_else(|| {
                format!("supersedesItemId {item_id} is not an approved, live item")
            })?;
            Some((item_id, item))
        }
    };
    let first_source = source_ticket_ids.first().copied();
    let is_merge = first_source.is_none();
    if is_merge && supersedes.is_none() {
        return Err("sourceTicketIds must not be empty".into());
    }
    let source_boards: HashSet<Uuid> = context
        .tickets
        .iter()
        .filter(|ticket| source_ticket_ids.contains(&ticket.id))
        .map(|ticket| ticket.board_id)
        .collect();

    let (board_id, agent_id) = match scope {
        KnowledgeScope::Workspace => (None, None),
        KnowledgeScope::Board | KnowledgeScope::Agent => {
            let board_id = match spec.board_id.as_deref() {
                Some(raw) => parse_id(raw, "boardId")?,
                None if is_merge => match supersedes.and_then(|(_, item)| item.board_id) {
                    Some(board) => board,
                    None => {
                        return Err("a merge of a workspace item must use workspace scope".into())
                    }
                },
                None if source_boards.len() == 1 => *source_boards.iter().next().unwrap(),
                None => return Err("boardId is required when sources span boards".into()),
            };
            if source_boards
                .iter()
                .any(|source_board| *source_board != board_id)
            {
                return Err(format!(
                    "boardId {board_id} does not match the board of every source ticket"
                ));
            }
            let agent_id = if scope == KnowledgeScope::Agent {
                let raw = spec
                    .agent_id
                    .as_deref()
                    .ok_or("agent scope requires agentId")?;
                let agent_id = parse_id(raw, "agentId")?;
                if !context.agent_ids.contains(&agent_id) {
                    return Err(format!("agent {agent_id} does not exist"));
                }
                Some(agent_id)
            } else {
                None
            };
            (Some(board_id), agent_id)
        }
    };

    let supersedes_item_id = match supersedes {
        None => None,
        Some((item_id, item)) => {
            let item_board = item.board_id;
            if item_board.is_some() && item_board != board_id {
                return Err(format!(
                    "supersedesItemId {item_id} belongs to a different board"
                ));
            }
            if is_merge && item_board.is_none() && scope != KnowledgeScope::Workspace {
                return Err("a merge of a workspace item must use workspace scope".into());
            }
            if is_merge && (scope != item.scope || agent_id != item.agent_id) {
                return Err(format!(
                    "a merge must keep the scope and agent of supersedesItemId {item_id}"
                ));
            }
            Some(item_id)
        }
    };

    let mut input = KnowledgeRevisionInput {
        scope,
        board_id,
        agent_id,
        knowledge_type,
        title: spec.title.clone(),
        content: spec.content.clone(),
        source_type: if is_merge {
            KnowledgeSourceType::AgentSummary
        } else {
            KnowledgeSourceType::Ticket
        },
        source_id: first_source,
        source_run_id: Some(context.run_id),
        confidence,
    };
    validate_revision(&mut input)?;

    let (citations, citation_notes) = review_citations(position, &spec.citations);
    let accepted = AcceptedCandidate {
        index: i32::try_from(position).unwrap_or(i32::MAX),
        input,
        source_ticket_ids,
        supersedes_item_id,
        requires_human_approval: spec.requires_human_approval,
        citations,
    };
    Ok((accepted, citation_notes))
}

/// Keep valid, distinct paths up to the cap. A bad citation never drops its
/// candidate; the reason goes back to the agent through the batch summary.
fn review_citations(position: usize, raw: &[String]) -> (Vec<String>, Vec<String>) {
    let mut kept: Vec<String> = Vec::new();
    let mut notes = Vec::new();
    let mut over_cap = 0usize;
    for item in raw {
        match validate_citation_path(item) {
            Ok(path) if kept.contains(&path) => {}
            Ok(_) if kept.len() >= MAX_CITATIONS_PER_CANDIDATE => over_cap += 1,
            Ok(path) => kept.push(path),
            Err(reason) => notes.push(format!(
                "candidate {}: citation `{item}` dropped: {reason}",
                position + 1
            )),
        }
    }
    if over_cap > 0 {
        notes.push(format!(
            "candidate {}: kept at most {MAX_CITATIONS_PER_CANDIDATE} citations; {over_cap} dropped",
            position + 1
        ));
    }
    (kept, notes)
}

fn parse_id(raw: &str, field: &str) -> Result<Uuid, String> {
    Uuid::parse_str(raw.trim()).map_err(|_| format!("{field} `{raw}` is not a valid id"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::knowledge::KnowledgeType;

    struct World {
        tickets: Vec<BatchTicket>,
        agents: HashSet<Uuid>,
        supersedable: HashMap<Uuid, SupersedableItem>,
    }

    impl World {
        fn new() -> Self {
            let board_a = Uuid::new_v4();
            let board_b = Uuid::new_v4();
            Self {
                tickets: vec![
                    BatchTicket {
                        id: Uuid::new_v4(),
                        board_id: board_a,
                    },
                    BatchTicket {
                        id: Uuid::new_v4(),
                        board_id: board_a,
                    },
                    BatchTicket {
                        id: Uuid::new_v4(),
                        board_id: board_b,
                    },
                ],
                agents: HashSet::from([Uuid::new_v4()]),
                supersedable: HashMap::new(),
            }
        }

        fn review(&self, specs: &[KnowledgeCandidateSpec], max: usize) -> CandidateReview {
            review_candidates(
                specs,
                &CandidateContext {
                    tickets: &self.tickets,
                    agent_ids: &self.agents,
                    supersedable: &self.supersedable,
                    run_id: Uuid::new_v4(),
                    max_candidates: max,
                },
            )
        }

        fn board_a(&self) -> Uuid {
            self.tickets[0].board_id
        }
    }

    fn supersedable(board_id: Option<Uuid>) -> SupersedableItem {
        SupersedableItem {
            scope: if board_id.is_some() {
                KnowledgeScope::Board
            } else {
                KnowledgeScope::Workspace
            },
            board_id,
            agent_id: None,
        }
    }

    fn spec(sources: &[Uuid]) -> KnowledgeCandidateSpec {
        KnowledgeCandidateSpec {
            knowledge_type: "test_command".into(),
            scope: "board".into(),
            title: "Run make test-unit while iterating".into(),
            content: "Use make test-unit; reserve make test for acceptance.".into(),
            confidence: Some("high".into()),
            source_ticket_ids: sources.iter().map(Uuid::to_string).collect(),
            ..KnowledgeCandidateSpec::default()
        }
    }

    #[test]
    fn accepts_a_board_candidate_and_infers_the_board() {
        let world = World::new();
        let review = world.review(&[spec(&[world.tickets[0].id, world.tickets[1].id])], 20);
        assert!(review.dropped.is_empty(), "{:?}", review.dropped);
        let accepted = &review.accepted[0];
        assert_eq!(accepted.index, 0);
        assert_eq!(accepted.input.board_id, Some(world.board_a()));
        assert_eq!(accepted.input.knowledge_type, KnowledgeType::TestCommand);
        assert_eq!(accepted.input.source_type, KnowledgeSourceType::Ticket);
        assert_eq!(accepted.input.source_id, Some(world.tickets[0].id));
        assert_eq!(accepted.source_ticket_ids.len(), 2);
    }

    #[test]
    fn bad_citation_is_dropped_but_candidate_kept() {
        let world = World::new();
        let mut with_citations = spec(&[world.tickets[0].id]);
        with_citations.citations = vec![
            "server/src/lib.rs".into(),
            "../../etc/passwd".into(),
            "server/src/lib.rs".into(),
        ];
        let review = world.review(&[with_citations], 20);
        assert_eq!(review.accepted.len(), 1);
        assert_eq!(review.accepted[0].citations, vec!["server/src/lib.rs"]);
        assert_eq!(review.dropped.len(), 1, "{:?}", review.dropped);
        assert!(
            review.dropped[0].contains("candidate 1"),
            "{:?}",
            review.dropped
        );
        assert!(
            review.dropped[0].contains("../../etc/passwd"),
            "{:?}",
            review.dropped
        );
    }

    #[test]
    fn citations_are_capped_at_ten_with_a_reason() {
        let world = World::new();
        let mut many = spec(&[world.tickets[0].id]);
        many.citations = (0..12).map(|n| format!("src/file{n}.rs")).collect();
        let review = world.review(&[many], 20);
        assert_eq!(review.accepted[0].citations.len(), 10);
        assert_eq!(review.accepted[0].citations[9], "src/file9.rs");
        assert_eq!(review.dropped.len(), 1, "{:?}", review.dropped);
        assert!(
            review.dropped[0].contains("at most 10"),
            "{:?}",
            review.dropped
        );
    }

    #[test]
    fn drops_invalid_sources_boards_types_and_bounds() {
        let world = World::new();
        let mut outside = spec(&[Uuid::new_v4()]);
        outside.title = "Outside".into();
        let empty = spec(&[]);
        let mut cross_board = spec(&[world.tickets[0].id, world.tickets[2].id]);
        cross_board.board_id = Some(world.board_a().to_string());
        let mut wrong_type = spec(&[world.tickets[0].id]);
        wrong_type.knowledge_type = "gossip".into();
        let mut blank_title = spec(&[world.tickets[0].id]);
        blank_title.title = "   ".into();
        let mut bad_agent = spec(&[world.tickets[0].id]);
        bad_agent.scope = "agent".into();
        bad_agent.agent_id = Some(Uuid::new_v4().to_string());
        let mut garbage_id = spec(&[world.tickets[0].id]);
        garbage_id.source_ticket_ids.push("not-a-uuid".into());

        let review = world.review(
            &[
                outside,
                empty,
                cross_board,
                wrong_type,
                blank_title,
                bad_agent,
                garbage_id,
            ],
            20,
        );
        assert!(review.accepted.is_empty());
        assert_eq!(review.dropped.len(), 7, "{:?}", review.dropped);
        assert!(review.dropped[0].contains("not in this batch"));
        assert!(review.dropped[1].contains("must not be empty"));
        assert!(review.dropped[2].contains("every source ticket"));
        assert!(review.dropped[3].contains("unknown type"));
        assert!(review.dropped[4].contains("title"));
        assert!(review.dropped[5].contains("does not exist"));
        assert!(review.dropped[6].contains("not a valid id"));
    }

    #[test]
    fn workspace_scope_clears_ids_and_supersession_is_checked() {
        let mut world = World::new();
        let live_item = Uuid::new_v4();
        let other_board_item = Uuid::new_v4();
        world.supersedable.insert(live_item, supersedable(None));
        world.supersedable.insert(
            other_board_item,
            supersedable(Some(world.tickets[2].board_id)),
        );

        let mut workspace = spec(&[world.tickets[0].id]);
        workspace.scope = "workspace".into();
        workspace.board_id = Some(world.board_a().to_string());
        workspace.supersedes_item_id = Some(live_item.to_string());
        let mut unknown = spec(&[world.tickets[0].id]);
        unknown.supersedes_item_id = Some(Uuid::new_v4().to_string());
        let mut other_board = spec(&[world.tickets[0].id]);
        other_board.supersedes_item_id = Some(other_board_item.to_string());

        let review = world.review(&[workspace, unknown, other_board], 20);
        assert_eq!(review.accepted.len(), 1, "{:?}", review.dropped);
        let accepted = &review.accepted[0];
        assert_eq!(accepted.input.scope, KnowledgeScope::Workspace);
        assert_eq!(accepted.input.board_id, None);
        assert_eq!(accepted.supersedes_item_id, Some(live_item));
        assert!(review.dropped[0].contains("not an approved, live item"));
        assert!(review.dropped[1].contains("different board"));
    }

    #[test]
    fn empty_sources_allowed_only_with_supersedes() {
        let mut world = World::new();
        let item = Uuid::new_v4();
        world
            .supersedable
            .insert(item, supersedable(Some(world.board_a())));

        // Direction 1: no sources and nothing superseded is still refused.
        let review = world.review(&[spec(&[])], 20);
        assert!(review.accepted.is_empty());
        assert!(
            review.dropped[0].contains("must not be empty"),
            "{:?}",
            review.dropped
        );

        // Direction 2: no sources but a superseded item is a merge.
        let mut merge = spec(&[]);
        merge.supersedes_item_id = Some(item.to_string());
        let review = world.review(&[merge], 20);
        assert!(review.dropped.is_empty(), "{:?}", review.dropped);
        let accepted = &review.accepted[0];
        assert!(accepted.source_ticket_ids.is_empty());
        assert_eq!(accepted.supersedes_item_id, Some(item));
        assert_eq!(accepted.input.board_id, Some(world.board_a()));
        assert_eq!(
            accepted.input.source_type,
            KnowledgeSourceType::AgentSummary
        );
        assert_eq!(accepted.input.source_id, None);

        // Sources plus supersedes keep the ticket source.
        let mut replace = spec(&[world.tickets[0].id]);
        replace.supersedes_item_id = Some(item.to_string());
        let review = world.review(&[replace], 20);
        let accepted = &review.accepted[0];
        assert_eq!(accepted.input.source_type, KnowledgeSourceType::Ticket);
        assert_eq!(accepted.input.source_id, Some(world.tickets[0].id));
    }

    #[test]
    fn merge_candidate_board_must_match_superseded_item() {
        let mut world = World::new();
        let board_item = Uuid::new_v4();
        let workspace_item = Uuid::new_v4();
        world
            .supersedable
            .insert(board_item, supersedable(Some(world.board_a())));
        world
            .supersedable
            .insert(workspace_item, supersedable(None));

        let mut wrong_board = spec(&[]);
        wrong_board.supersedes_item_id = Some(board_item.to_string());
        wrong_board.board_id = Some(world.tickets[2].board_id.to_string());
        let mut inferred = spec(&[]);
        inferred.supersedes_item_id = Some(board_item.to_string());
        let mut board_scope_over_workspace_item = spec(&[]);
        board_scope_over_workspace_item.supersedes_item_id = Some(workspace_item.to_string());
        board_scope_over_workspace_item.board_id = Some(world.board_a().to_string());
        let mut workspace_merge = spec(&[]);
        workspace_merge.scope = "workspace".into();
        workspace_merge.supersedes_item_id = Some(workspace_item.to_string());

        let review = world.review(
            &[
                wrong_board,
                inferred,
                board_scope_over_workspace_item,
                workspace_merge,
            ],
            20,
        );
        assert_eq!(review.accepted.len(), 2, "{:?}", review.dropped);
        assert_eq!(review.dropped.len(), 2, "{:?}", review.dropped);
        assert!(
            review.dropped[0].contains("different board"),
            "{:?}",
            review.dropped
        );
        assert!(
            review.dropped[1].contains("workspace"),
            "{:?}",
            review.dropped
        );
        assert_eq!(review.accepted[0].input.board_id, Some(world.board_a()));
        assert_eq!(review.accepted[1].input.scope, KnowledgeScope::Workspace);
        assert_eq!(review.accepted[1].input.board_id, None);
    }

    #[test]
    fn merge_must_keep_the_scope_and_agent_of_the_superseded_item() {
        let mut world = World::new();
        let agent = *world.agents.iter().next().unwrap();
        let other_agent = Uuid::new_v4();
        world.agents.insert(other_agent);
        let agent_item = Uuid::new_v4();
        let board_item = Uuid::new_v4();
        world.supersedable.insert(
            agent_item,
            SupersedableItem {
                scope: KnowledgeScope::Agent,
                board_id: Some(world.board_a()),
                agent_id: Some(agent),
            },
        );
        world
            .supersedable
            .insert(board_item, supersedable(Some(world.board_a())));

        let merge = |item: Uuid, scope: &str, agent_id: Option<Uuid>| {
            let mut candidate = spec(&[]);
            candidate.supersedes_item_id = Some(item.to_string());
            candidate.scope = scope.into();
            candidate.agent_id = agent_id.map(|id| id.to_string());
            candidate
        };
        let review = world.review(
            &[
                merge(agent_item, "agent", Some(other_agent)),
                merge(agent_item, "board", None),
                merge(board_item, "agent", Some(agent)),
                merge(agent_item, "agent", Some(agent)),
            ],
            20,
        );
        assert_eq!(review.dropped.len(), 3, "{:?}", review.dropped);
        for reason in &review.dropped {
            assert!(reason.contains("scope and agent"), "{reason}");
        }
        assert_eq!(review.accepted.len(), 1);
        assert_eq!(review.accepted[0].index, 3);
        assert_eq!(review.accepted[0].input.agent_id, Some(agent));
    }

    #[test]
    fn caps_candidates_per_batch_and_keeps_original_indexes() {
        let world = World::new();
        let mut invalid = spec(&[]);
        invalid.title = "invalid".into();
        let specs = vec![
            invalid,
            spec(&[world.tickets[0].id]),
            spec(&[world.tickets[1].id]),
            spec(&[world.tickets[0].id]),
        ];
        let review = world.review(&specs, 2);
        assert_eq!(
            review
                .accepted
                .iter()
                .map(|candidate| candidate.index)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(review.dropped.len(), 2);
        assert!(review.dropped[1].contains("over the limit"));
    }

    #[test]
    fn parses_the_wire_format_leniently() {
        let parsed: Vec<KnowledgeCandidateSpec> = serde_json::from_value(serde_json::json!([
            {
                "type": "bug_pattern",
                "scope": "board",
                "boardId": null,
                "title": "t",
                "content": "c",
                "sourceTicketIds": ["x"],
                "requiresHumanApproval": true,
                "unknownField": 1
            },
            {}
        ]))
        .unwrap();
        assert_eq!(parsed[0].knowledge_type, "bug_pattern");
        assert!(parsed[0].requires_human_approval);
        assert_eq!(parsed[1], KnowledgeCandidateSpec::default());
    }
}
