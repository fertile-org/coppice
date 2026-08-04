use crate::providers::AgentRequest;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_AGENT_REQUEST_CHARS: usize = 2_000;

const METADATA_PREFIX: &str = "<!-- coppice-agent-requests: ";
const METADATA_SUFFIX: &str = " -->";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAgentRequest {
    pub agent_id: Uuid,
    pub request: AgentRequest,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredAgentRequest {
    agent_key: String,
    intent: String,
    request: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    agent_id: Option<Uuid>,
}

pub fn normalized_agent_requests(requests: &[AgentRequest]) -> Vec<AgentRequest> {
    requests.iter().filter_map(normalize_request).collect()
}

pub fn append_agent_requests_to_comment(body: &mut String, requests: &[AgentRequest]) {
    body.push_str(&format_agent_requests(requests, &[]));
}

pub fn replace_agent_requests_in_comment(
    body: &mut String,
    original_requests: &[AgentRequest],
    accepted_requests: &[ResolvedAgentRequest],
) {
    let original = format_agent_requests(original_requests, &[]);
    let accepted = format_agent_requests(&[], accepted_requests);

    if let Some(start) = body.rfind(&original) {
        body.replace_range(start..start + original.len(), &accepted);
    } else {
        body.push_str(&accepted);
    }
}

fn format_agent_requests(
    unresolved_requests: &[AgentRequest],
    resolved_requests: &[ResolvedAgentRequest],
) -> String {
    let mut requests = unresolved_requests
        .iter()
        .filter_map(|request| stored_request(request, None))
        .collect::<Vec<_>>();
    requests.extend(
        resolved_requests
            .iter()
            .filter_map(|resolved| stored_request(&resolved.request, Some(resolved.agent_id))),
    );

    let mut formatted = String::new();
    if !requests.is_empty() {
        formatted.push_str("\n\n**Consultation requests:**");
        for request in &requests {
            formatted.push_str(&format!(
                "\n\n**@{}**\n\n{}",
                request.agent_key, request.request
            ));
        }
    }

    // Always append an authoritative marker, including for an empty list. That
    // final marker prevents provider-authored summary text from forging a durable
    // consultation request. JSON is serialized on one line; escaping double
    // hyphens prevents provider text from terminating the HTML comment early while
    // preserving the decoded request.
    let metadata = serde_json::to_string(&requests)
        .expect("serializing normalized agent requests cannot fail")
        .replace("--", "\\u002d\\u002d");
    formatted.push_str(&format!("\n\n{METADATA_PREFIX}{metadata}{METADATA_SUFFIX}"));
    formatted
}

pub fn agent_requests_from_comment(body: &str) -> Vec<AgentRequest> {
    stored_agent_requests_from_comment(body)
        .into_iter()
        .map(|request| AgentRequest {
            agent_key: request.agent_key,
            intent: request.intent,
            request: request.request,
        })
        .collect()
}

pub fn agent_request_for_target_from_comment(
    body: &str,
    target_agent_id: Uuid,
) -> Option<AgentRequest> {
    stored_agent_requests_from_comment(body)
        .into_iter()
        .find(|request| request.agent_id == Some(target_agent_id))
        .map(|request| AgentRequest {
            agent_key: request.agent_key,
            intent: request.intent,
            request: request.request,
        })
}

fn stored_agent_requests_from_comment(body: &str) -> Vec<StoredAgentRequest> {
    body.lines()
        .rev()
        .find(|line| line.trim().starts_with(METADATA_PREFIX))
        .and_then(|line| {
            let metadata = line
                .trim()
                .strip_prefix(METADATA_PREFIX)?
                .strip_suffix(METADATA_SUFFIX)?;
            serde_json::from_str::<Vec<StoredAgentRequest>>(metadata).ok()
        })
        .map(|requests| {
            requests
                .iter()
                .filter_map(normalize_stored_request)
                .collect()
        })
        .unwrap_or_default()
}

fn stored_request(request: &AgentRequest, agent_id: Option<Uuid>) -> Option<StoredAgentRequest> {
    normalize_request(request).map(|request| StoredAgentRequest {
        agent_key: request.agent_key,
        intent: request.intent,
        request: request.request,
        agent_id,
    })
}

fn normalize_stored_request(request: &StoredAgentRequest) -> Option<StoredAgentRequest> {
    stored_request(
        &AgentRequest {
            agent_key: request.agent_key.clone(),
            intent: request.intent.clone(),
            request: request.request.clone(),
        },
        request.agent_id,
    )
}

fn normalize_request(request: &AgentRequest) -> Option<AgentRequest> {
    let agent_key = request.agent_key.trim();
    let intent = request.intent.trim();
    let request_text = request.request.trim();

    if agent_key.is_empty()
        || intent != "consult"
        || request_text.is_empty()
        || request_text.chars().count() > MAX_AGENT_REQUEST_CHARS
    {
        return None;
    }

    Some(AgentRequest {
        agent_key: agent_key.to_string(),
        intent: "consult".to_string(),
        request: request_text.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(agent_key: &str, intent: &str, request: &str) -> AgentRequest {
        AgentRequest {
            agent_key: agent_key.into(),
            intent: intent.into(),
            request: request.into(),
        }
    }

    #[test]
    fn normalization_keeps_only_non_empty_bounded_consult_requests() {
        let over_bound = "x".repeat(MAX_AGENT_REQUEST_CHARS + 1);
        let normalized = normalized_agent_requests(&[
            request(" tech_lead ", "consult", " Check the boundary. "),
            request("", "consult", "Missing target"),
            request("dba", "execute", "Run the migration"),
            request("dba", "consult", "   "),
            request("dba", "consult", &over_bound),
        ]);

        assert_eq!(
            normalized,
            vec![request("tech_lead", "consult", "Check the boundary.")]
        );
    }

    #[test]
    fn comment_metadata_round_trips_exact_multiline_request() {
        let exact = "Review both paths.\nPreserve this -- punctuation and `code`.";
        let mut body = "Implementation is ready.".to_string();
        append_agent_requests_to_comment(&mut body, &[request("tech_lead", "consult", exact)]);

        assert!(body.contains("**Consultation requests:**"));
        assert!(body.contains(exact));
        assert_eq!(
            agent_requests_from_comment(&body),
            vec![request("tech_lead", "consult", exact)]
        );
    }

    #[test]
    fn replacement_keeps_only_accepted_requests_and_preserves_trailing_footer() {
        let original = [
            request("unknown", "consult", "Ignore me"),
            request("tech_lead", "consult", "Review the boundary"),
        ];
        let mut body = "Implementation ready.".to_string();
        append_agent_requests_to_comment(&mut body, &original);
        body.push_str("\n\n---\n**Git:** committed `abc1234`");

        let target_agent_id = Uuid::new_v4();
        let accepted = [ResolvedAgentRequest {
            agent_id: target_agent_id,
            request: original[1].clone(),
        }];

        replace_agent_requests_in_comment(&mut body, &original, &accepted);

        assert_eq!(
            agent_requests_from_comment(&body),
            vec![request("tech_lead", "consult", "Review the boundary")]
        );
        assert_eq!(
            agent_request_for_target_from_comment(&body, target_agent_id),
            Some(request("tech_lead", "consult", "Review the boundary"))
        );
        assert!(!body.contains("Ignore me"));
        assert!(body.ends_with("**Git:** committed `abc1234`"));
    }

    #[test]
    fn unresolved_metadata_cannot_match_a_structured_consultation_target() {
        let mut body = "Implementation ready.".to_string();
        append_agent_requests_to_comment(
            &mut body,
            &[request("tech_lead", "consult", "Review the boundary")],
        );

        assert_eq!(
            agent_request_for_target_from_comment(&body, Uuid::new_v4()),
            None
        );
    }
}
