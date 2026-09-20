import type { KnowledgeType } from '../../lib/schemas/knowledge';

/** Litmus for Pending Inbox curation — exact ticket wording. */
export const CURATION_LITMUS =
  'Would a different ticket next month still need this exact rule?';

/** Trust framing: approval = retrieval eligibility, not instruction authority. */
export const CURATION_TRUST_FRAMING =
  'Approval makes content eligible for retrieval as untrusted reference data — not instruction authority.';

export interface RejectPreset {
  id: 'one_off' | 'too_vague' | 'wrong_scope' | 'duplicate';
  label: string;
  /** Human-readable audit string stored as rejectionReason. */
  reasonText: string;
}

export const REJECT_PRESETS: RejectPreset[] = [
  {
    id: 'one_off',
    label: 'One-off',
    reasonText: 'One-off for this ticket; not reusable next month.',
  },
  {
    id: 'too_vague',
    label: 'Too vague',
    reasonText: 'Too vague to apply reliably; needs clearer wording.',
  },
  {
    id: 'wrong_scope',
    label: 'Wrong scope',
    reasonText: 'Wrong scope; belongs elsewhere or is too broad/narrow.',
  },
  {
    id: 'duplicate',
    label: 'Duplicate',
    reasonText: 'Duplicate of existing approved knowledge.',
  },
];

export interface TypeGuidance {
  approveExample: string;
  rejectExample: string;
}

const FALLBACK_GUIDANCE: TypeGuidance = {
  approveExample: 'Approve: durable, scoped fact another ticket would reuse.',
  rejectExample: 'Reject: one-off detail, vague advice, or wrong scope.',
};

const TYPE_GUIDANCE: Partial<Record<KnowledgeType, TypeGuidance>> = {
  coding_convention: {
    approveExample: 'Approve: "Prefer Result over panic in public APIs."',
    rejectExample: 'Reject: "Rename this helper in the PR."',
  },
  architecture_rule: {
    approveExample: 'Approve: "Handlers stay thin; rules live in services."',
    rejectExample: 'Reject: "Move this file for this refactor."',
  },
  bug_pattern: {
    approveExample: 'Approve: "Null CSRF on mutations causes 403s."',
    rejectExample: 'Reject: "This ticket had a null pointer."',
  },
  test_command: {
    approveExample: 'Approve: "Use make test-unit while iterating."',
    rejectExample: 'Reject: "Re-run the failing test from this run."',
  },
  review_feedback: {
    approveExample: 'Approve: "Cite line ranges in review comments."',
    rejectExample: 'Reject: "LGTM on this PR."',
  },
  dependency_note: {
    approveExample: 'Approve: "sqlx 0.8 requires Tokio runtime."',
    rejectExample: 'Reject: "Bump this crate for the ticket."',
  },
  api_contract: {
    approveExample: 'Approve: "Reject body uses { expectedVersion, reason }."',
    rejectExample: 'Reject: "Fix this endpoint for the bug."',
  },
  workflow_rule: {
    approveExample: 'Approve: "Done tickets need human gate before merge."',
    rejectExample: 'Reject: "Move this card to In Review now."',
  },
  human_preference: {
    approveExample: 'Approve: "Prefer concise commit messages."',
    rejectExample: 'Reject: "I liked this diff today."',
  },
  operational_runbook: {
    approveExample: 'Approve: "Compose stack uses ports 5432/5000/5001."',
    rejectExample: 'Reject: "Restart the container for this outage."',
  },
  security_rule: {
    approveExample: 'Approve: "Mutations require X-CSRF-Token."',
    rejectExample: 'Reject: "Rotate this leaked token."',
  },
  performance_note: {
    approveExample: 'Approve: "Avoid N+1 on knowledge list pages."',
    rejectExample: 'Reject: "This query was slow once."',
  },
};

export function guidanceForType(type: string | undefined | null): TypeGuidance {
  if (type && type in TYPE_GUIDANCE) {
    return TYPE_GUIDANCE[type as KnowledgeType] ?? FALLBACK_GUIDANCE;
  }
  return FALLBACK_GUIDANCE;
}
