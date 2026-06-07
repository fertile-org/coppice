# Agent Workspace Product Design

## 1. Product Summary

This product is a lightweight, self-hosted workspace for managing AI agents as engineering teammates. It combines a Trello-like board, ticket comments, agent assignment, live terminal sessions, controlled long-term memory, sandboxed capabilities, secrets, and proactive agent signals.

The product is not intended to be a heavy Jira replacement or a generic chat-based multi-agent playground. It is a practical control plane for AI coding and engineering agents that work through visible tickets, communicate through comments, operate inside explicit sandboxes, learn reusable project knowledge, and stop at human approval gates.

The intended user is a solo senior engineer, small engineering team, or self-hosted AI-heavy development workflow that uses tools such as Claude Code, Codex CLI, OpenCode, OpenClaw, or similar CLI-based coding agents.

The core product identity:

```text
A lightweight Trello-like workspace where AI agents can work on assigned tickets, ask each other questions, raise proactive engineering concerns, use bounded tools and credentials, learn project knowledge, and stop at human final review.
```

## 2. Core Differentiation

The product is different from a simple multi-agent Kanban board because it treats agents as bounded role owners, not only task executors.

Most agent boards follow this model:

```text
Human creates task -> agent works -> agent reports result
```

This product supports:

```text
Agent owns a domain -> agent observes its domain -> agent raises concern -> human converts concern into ticket -> workflow starts
```

For example, a DBA Agent may be the only agent with readonly database access. It can periodically inspect database health, notice that the current standalone database is becoming risky, and proactively notify the workspace owner that this technical debt should be addressed.

The product's strongest values are:

1. Workflow-first engineering process, not only agent assignment.
2. Ticket comments as the official inter-agent communication protocol.
3. Live terminal visibility for CLI-based agents.
4. Controlled self-learning through typed knowledge and pgvector retrieval.
5. Capability-based sandboxing and secret access.
6. Proactive workspace signals from role-owner agents.
7. Human final review as a first-class safety gate.

## 3. Product Principles

### 3.1 Simplicity Over Enterprise Complexity

The UI should feel closer to Trello than Jira. The system should have simple boards, cards, comments, attachments, and obvious actions.

Avoid complex project-management concepts unless they directly support agent workflow.

### 3.2 Observable Work Over Hidden Magic

Every agent action should be visible through ticket comments, run logs, live terminal sessions, artifacts, and state changes.

Agents should not communicate through hidden chat sessions that humans cannot inspect.

### 3.3 Controlled Autonomy

Agents can work, ask questions, raise blockers, and proactively create signals. However, sensitive actions such as granting credentials, pushing code, creating production changes, or final approval require human involvement.

### 3.4 Sandboxed Confidence

Agents should be more confident when they know exactly what tools, commands, paths, secrets, and network destinations they can access.

If an agent lacks a required capability, it should explicitly create a blocker instead of guessing or continuing blindly.

### 3.5 Memory Hygiene

Agents should learn from work progress, but not by saving everything forever. Knowledge must be typed, scoped, filtered, ranked, expired, and auditable.

## 4. Main Product Areas

The product has six main areas:

1. Board and tickets.
2. Agent management.
3. Agent runs and live sessions.
4. Comments, mentions, and blockers.
5. Knowledge and self-learning.
6. Capabilities, sandbox profiles, secrets, and proactive signals.

## 5. Board and Ticket System

### 5.1 Board Columns

The board should be simple and fixed for the first version:

```text
Backlog
Ready
In Progress
In Review
In QA
Wait for Final Review
Done
Blocked
```

Internal substatus can be richer while keeping the board simple.

Examples:

```text
waiting_for_pm_agent
waiting_for_tech_lead
waiting_for_engineer
waiting_for_human
waiting_for_owner
waiting_for_ci
blocked_by_missing_capability
blocked_by_missing_secret
blocked_by_permission
blocked_by_error
```

The UI may display a card as:

```text
In Progress
Waiting for PM Agent
```

or:

```text
Blocked
Needs owner to grant DB_READONLY_URL
```

### 5.2 Ticket Fields

A ticket should contain:

```ts
Ticket {
  id: string;
  projectId: string;
  repoId?: string;

  title: string;
  description: string;

  status: string;
  substatus?: string;
  priority?: "low" | "medium" | "high" | "critical";

  assigneeAgentId?: string;
  reviewerAgentId?: string;
  ownerUserId?: string;

  branchName?: string;
  worktreePath?: string;

  parentTicketId?: string;
  sourceSignalId?: string;

  createdBy: "human" | "agent" | "system";
  createdById?: string;

  createdAt: Date;
  updatedAt: Date;
}
```

### 5.3 Ticket Card UI

A board card should show only the essential information:

```text
Title
Assignee agent
Status/substatus badge
Repo/branch badge
Last activity
Blocked/waiting badge
Live run badge
Screenshot/attachment badge
```

### 5.4 Ticket Detail UI

Ticket detail is the most important screen.

Recommended tabs:

```text
Description
Comments
Live Console
Artifacts
Runs
Knowledge Used
Metadata
```

Recommended actions:

```text
Assign Agent
Run Agent
Stop Run
Retry
Move Status
Create Worktree
View Diff
Push Branch
Create PR
Final Approve
Convert Signal to Ticket
Resolve Blocker
```

## 6. Agent Management

### 6.1 Agent as Configurable Role Owner

An agent is not only a model. It is a role with personality, skills, responsibilities, tools, capabilities, sandbox profile, secrets, workflow rules, and memory scope.

```ts
Agent {
  id: string;
  name: string;

  role: string;
  personalityPreset?: string;
  skills: string[];
  responsibilities: string[];

  systemPrompt: string;
  providerId: string;
  modelConfig?: object;

  capabilityIds: string[];
  sandboxProfileId: string;
  allowedSecretIds: string[];

  maxConcurrentTasks: number;
  proactiveEnabled: boolean;
  observationSchedule?: string;

  enabled: boolean;
  createdAt: Date;
  updatedAt: Date;
}
```

### 6.2 Agent Presets

The system should provide default presets but allow customization.

Recommended presets:

```text
PM Agent
Technical Lead Agent
Frontend Engineer Agent
Backend Engineer Agent
DBA Agent
QC Agent
Reviewer Agent
DevOps Agent
Security Agent
Research Agent
```

Recommended personality presets:

```text
Pragmatic senior engineer
Strict reviewer
Minimal-change engineer
Security-focused engineer
Performance-focused engineer
UX-focused QC
Fast prototyper
Careful planner
```

### 6.3 Agent Responsibilities

Responsibilities define what an agent owns and what proactive signals it is allowed to raise.

Example DBA Agent:

```yaml
role: DBA
responsibilities:
  - monitor database health
  - inspect slow queries
  - detect backup and replication risks
  - suggest index and schema improvements
  - raise database capacity and safety concerns
allowed_signal_types:
  - performance_concern
  - tech_debt
  - operational_risk
  - maintenance_reminder
```

Example Frontend Engineer Agent:

```yaml
role: Frontend Engineer
responsibilities:
  - implement frontend tickets
  - follow project UI architecture
  - fix QC-reported frontend bugs
  - raise frontend tech debt or repeated UI regression concerns
allowed_signal_types:
  - tech_debt
  - bug_pattern
  - maintainability_concern
```

## 7. Agent Provider Abstraction

The product should not depend on one model API. It should run existing CLI tools.

Supported provider types:

```text
Claude Code CLI
Codex CLI
OpenCode CLI
OpenClaw agent
Shell command provider
Future custom providers
```

Provider config example:

```yaml
providers:
  claude-code:
    command: "claude"
    default_args: []

  codex:
    command: "codex"
    default_args: []

  opencode:
    command: "opencode"
    default_args: []
    env:
      OPENAI_API_KEY: "${OPENAI_API_KEY}"
```

The core product should only know that an agent provider can execute an agent run with a context package and return output.

Conceptual interface:

```ts
interface AgentProvider {
  run(input: AgentRunInput): Promise<AgentRunResult>;
}
```

## 8. Workflow Model

### 8.1 Workflow as State Machine

The product should use explicit workflow rules instead of relying on agents to decide every transition.

Example flow:

```text
PM Agent creates/refines ticket
  -> Technical Lead Agent plans/reviews technical direction
  -> FE/BE Engineer Agent implements
  -> Technical Lead Agent reviews
  -> QC Agent tests
  -> Wait for Final Review
  -> Human approves
  -> Done
```

### 8.2 Workflow Rule Example

```yaml
workflow:
  on_ticket_created:
    if:
      status: Backlog
      label: product-request
    then:
      assign_to: pm-agent

  on_agent_done:
    pm-agent:
      move_to: Ready
      assign_to: tech-lead-agent

    tech-lead-agent:
      move_to: In Progress
      assign_to_any:
        - frontend-engineer-agent
        - backend-engineer-agent

    frontend-engineer-agent:
      move_to: In Review
      mention: tech-lead-agent

    backend-engineer-agent:
      move_to: In Review
      mention: tech-lead-agent

    tech-lead-review-approved:
      move_to: In QA
      assign_to: qc-agent

    qc-agent-approved:
      move_to: Wait for Final Review
      assign_to: human
```

### 8.3 Human Final Review

The system must treat human final review as a first-class gate.

Agents may prepare branches, diffs, summaries, test reports, and PR descriptions, but human should control final approval and merge by default.

## 9. Comments, Mentions, and Inter-Agent Communication

### 9.1 Comments as the Official Communication Protocol

Agents communicate through ticket comments, not hidden chats.

A comment should support:

```ts
TicketComment {
  id: string;
  ticketId: string;

  authorType: "human" | "agent" | "system";
  authorId?: string;

  body: string;
  intent:
    | "progress_update"
    | "clarification_request"
    | "clarification_answer"
    | "review_feedback"
    | "bug_report"
    | "implementation_done"
    | "qa_failed"
    | "qa_passed"
    | "blocked"
    | "system_event";

  mentions: string[];
  attachments: Attachment[];

  createdAt: Date;
}
```

### 9.2 Mention System

When an agent comments with `@agent-name`, the system creates a mention and a job for the mentioned agent.

```ts
Mention {
  id: string;
  ticketId: string;
  commentId: string;
  mentionedType: "agent" | "human" | "owner";
  mentionedId: string;
  status: "pending" | "handled" | "ignored";
  createdAt: Date;
}
```

Example:

```text
FE Agent:
@pm-agent Should the empty state show a CTA or plain message?

System:
Created job for PM Agent.
Ticket moved to Waiting for PM.
```

### 9.3 Clarification Flow

Agents should stop and ask questions when requirements are unclear.

Example result:

```json
{
  "status": "blocked",
  "reason": "Requirement unclear",
  "comment": "@pm-agent Should the retry error show immediately or only after final retry fails?",
  "nextStatus": "Waiting for PM",
  "mentionAgents": ["pm-agent"]
}
```

The original agent should not wait forever in the same live session. It should stop cleanly. After the PM responds, the workflow creates a resume job for the original agent.

### 9.4 Communication Limits

To avoid infinite loops:

```yaml
communication_limits:
  max_clarification_rounds_per_ticket: 3
  max_agent_mentions_per_run: 2
  max_auto_resume_count: 3
```

After limits are reached, escalate to human.

## 10. Agent Runs and Live Console

### 10.1 Live Console

Because the product depends on CLI tools such as Claude Code, Codex, OpenCode, and OpenClaw, the simplest reliable observability mechanism is a terminal session.

The product should provide a Live Console for each agent run.

```text
Agent Run
  -> tmux or PTY session
  -> CLI process
  -> terminal output stream
  -> WebSocket
  -> browser terminal UI
```

The UI should call this feature:

```text
Live Console
Agent Console
Live Session
```

Do not call it “Thinking”, because the system is observing terminal output, tool usage, and work trail, not guaranteed private model reasoning.

### 10.2 Agent Run Fields

```ts
AgentRun {
  id: string;
  ticketId?: string;
  signalId?: string;
  agentId: string;

  jobType: string;
  status:
    | "queued"
    | "running"
    | "waiting_input"
    | "blocked"
    | "succeeded"
    | "failed"
    | "cancelled";

  sessionId?: string;
  sandboxProfileId: string;
  worktreePath?: string;

  retrievedKnowledgeIds: string[];
  usedSecretIds: string[];
  usedCapabilityIds: string[];

  startedAt?: Date;
  endedAt?: Date;
}
```

### 10.3 Terminal Logs

Terminal output should be streamed live and saved as an artifact.

Do not store large terminal logs directly in the database. Store them on filesystem/object storage and keep metadata in the database.

## 11. Worktree Management

Each implementation ticket should use an isolated git worktree.

Example layout:

```text
/repos/my-app
/worktrees/TICKET-123-my-app
```

Example lifecycle:

```text
Create ticket
  -> create branch agent/TICKET-123
  -> create worktree
  -> run agent in worktree
  -> agent commits locally
  -> attach diff summary
  -> human reviews
  -> optional push/create PR
```

By default, agents should not push or merge unless explicitly allowed.

## 12. Artifacts and Attachments

Agents and humans can attach artifacts to comments and runs.

Artifact types:

```text
image
screenshot
file
terminal_log
diff
test_report
review_report
summary
agent_result
```

QC Agent example:

```text
@frontend-agent The save button remains clickable during loading. Screenshot attached.
```

The screenshot is stored as an attachment. A short textual summary may be embedded into knowledge later, but the raw image should remain an artifact.

## 13. Knowledge and Self-Learning

### 13.1 Goal

Agents should learn across tasks, but memory must be controlled. The system should avoid the common problem where agent memory grows forever and makes future context noisy.

The product should support:

```text
Controlled learning
Curated memory
Typed knowledge
Scoped retrieval
Context budget
Memory approval
Usage tracking
Expiry and consolidation
```

### 13.2 Memory Layers

Recommended memory layers:

```text
Ticket context
Project knowledge
Agent-specific memory
Team memory
Human-approved memory
```

### 13.3 Knowledge Item Model

```ts
KnowledgeItem {
  id: string;

  scope: "agent" | "project" | "team" | "workspace";
  projectId?: string;
  agentId?: string;

  type:
    | "coding_convention"
    | "architecture_rule"
    | "bug_pattern"
    | "test_command"
    | "review_feedback"
    | "dependency_note"
    | "api_contract"
    | "workflow_rule"
    | "human_preference"
    | "operational_runbook"
    | "security_rule"
    | "performance_note";

  title: string;
  content: string;

  sourceType:
    | "ticket"
    | "comment"
    | "review"
    | "human_note"
    | "agent_summary"
    | "workspace_signal"
    | "observation_run";
  sourceId?: string;

  confidence: "low" | "medium" | "high";
  approvedByHuman: boolean;

  usageCount: number;
  lastUsedAt?: Date;
  expiresAt?: Date;
  supersededBy?: string;

  createdAt: Date;
  updatedAt: Date;
}
```

### 13.4 Vector Storage

The product should use PostgreSQL with pgvector from v1.

Knowledge retrieval needs relational filters plus vector similarity:

```text
project_id
agent_id
scope
type
confidence
approval status
expiry
superseded status
vector similarity
usage signals
```

### 13.5 Learning Extractor

After a ticket is completed or a significant signal is resolved, a Learning Extractor should propose candidate knowledge.

Flow:

```text
Completed work
  -> read ticket, comments, review, test report, final human feedback
  -> propose candidate knowledge
  -> auto-save low-risk items or send to Knowledge Inbox
  -> embed approved/saved knowledge
```

Example candidate:

```json
{
  "type": "coding_convention",
  "title": "Use server-state facade for API state",
  "content": "Feature code should call the server-state facade instead of using React Query directly in screen components.",
  "scope": "project",
  "confidence": "high",
  "shouldRequireHumanApproval": false
}
```

### 13.6 Knowledge Inbox

The UI should include a Knowledge Inbox:

```text
Pending
Approved
Rejected
Stale
```

Actions:

```text
Approve
Edit
Reject
Supersede
Mark stale
```

### 13.7 Context Control

The context builder should have a strict budget.

Example:

```yaml
context_budget:
  max_tokens: 24000

sections:
  ticket: 5000
  latest_comments: 4000
  project_rules: 3000
  retrieved_knowledge: 4000
  previous_attempt_summary: 2000
  output_contract: 1000
```

Retrieval should use metadata filters first, then vector search, then reranking.

## 14. Capabilities, Sandbox Profiles, and Secrets

### 14.1 Why This Is Core

The product should allow agents to have different access levels.

Example:

```text
DBA Agent has readonly database access.
FE Agent has repo/worktree access but no DB access.
Security Agent has dependency scan access.
QC Agent can run browser tests and attach screenshots.
```

This requires a sandbox and capability model.

### 14.2 Capability Model

Capabilities are high-level access bundles that map to commands, secrets, filesystem paths, and network access.

```ts
Capability {
  id: string;
  name: string;
  description: string;

  requiredCommands: string[];
  requiredSecrets: string[];
  requiredNetworkHosts: string[];
  requiredPaths: string[];

  riskLevel: "low" | "medium" | "high" | "critical";
  readonly: boolean;
}
```

Example:

```yaml
capabilities:
  postgres_readonly_inspection:
    description: "Inspect PostgreSQL health using readonly credentials"
    required_commands:
      - psql
      - pg_isready
    required_secrets:
      - DB_READONLY_URL
    required_network_hosts:
      - postgres.internal
    readonly: true
    risk_level: medium
```

### 14.3 Sandbox Profile

A sandbox profile defines the low-level execution boundary for an agent run.

```ts
SandboxProfile {
  id: string;
  name: string;

  allowedCommands: string[];
  deniedCommands: string[];

  allowedPaths: string[];
  deniedPaths: string[];

  allowedNetworkHosts: string[];
  deniedNetworkHosts: string[];

  allowedSecretIds: string[];

  resourceLimits: {
    cpu?: string;
    memoryMb?: number;
    timeoutMinutes?: number;
    maxOutputMb?: number;
  };
}
```

Example DBA profile:

```yaml
sandbox_profiles:
  dba-readonly:
    allowed_commands:
      - psql
      - pg_isready
      - bash
      - cat
      - grep
      - awk
      - sed
    denied_commands:
      - rm
      - curl
      - wget
      - ssh
      - scp
    allowed_secrets:
      - DB_READONLY_URL
    allowed_network_hosts:
      - postgres.internal
    resource_limits:
      memory_mb: 512
      timeout_minutes: 20
```

### 14.4 Sandbox Implementation Levels

The product should support a simple v1 and allow stronger isolation later.

#### v1: Process-Level Sandbox

```text
Run as controlled process
Controlled working directory
Restricted environment variables
Command allowlist wrapper
Timeouts
Log everything
No secrets unless explicitly injected
```

This is useful but should not be described as a perfect security boundary.

#### v2: Container Sandbox

```text
One container per run
Mounted worktree
Injected secrets
Resource limits
Network restrictions if possible
```

#### v3: Strong Sandbox

```text
Rootless containers
bubblewrap/nsjail
Firecracker/microVM
network egress policy
strong filesystem isolation
```

### 14.5 Secret Management

Agents should not receive raw secrets in prompts or comments. Secrets should be stored securely and injected into the sandbox only when allowed.

```ts
Secret {
  id: string;
  name: string;
  scope: "workspace" | "project" | "agent";
  encryptedValue: string;

  allowedAgentIds: string[];
  allowedSandboxProfileIds: string[];

  createdAt: Date;
  rotatedAt?: Date;
}
```

Agents should reference secret names, not values.

Example blocker:

```text
DBA Agent is missing required secret: DB_READONLY_URL
```

Not:

```text
DBA Agent needs postgres://user:password@host/db
```

### 14.6 Capability Blocker Flow

If an agent cannot do its job because it lacks a command, secret, network destination, or path, it should create a blocker.

Example result:

```json
{
  "status": "blocked",
  "blockerType": "missing_capability",
  "requiredCapability": "postgres_readonly_inspection",
  "requiredCommand": "psql",
  "requiredSecret": "DB_READONLY_URL",
  "message": "@owner I need readonly PostgreSQL access to inspect slow queries. Please allow `psql` and provide `DB_READONLY_URL`.",
  "nextStatus": "Waiting for Owner"
}
```

The UI should show guided unblock actions:

```text
Allow command
Add secret
Grant capability
Allow network host
Reject request
Ask agent why
```

## 15. Proactive Agents and Workspace Signals

### 15.1 Concept

Agents should be able to raise concerns without being assigned a task, but only inside their responsibility scope.

This turns agents into bounded domain owners.

Examples:

```text
DBA Agent raises database capacity risk.
Security Agent raises dependency vulnerability concern.
DevOps Agent raises CI cost/performance concern.
QC Agent raises repeated regression pattern.
FE Agent raises frontend maintainability concern.
```

### 15.2 Workspace Signal Model

```ts
WorkspaceSignal {
  id: string;
  workspaceId: string;
  projectId?: string;

  createdByAgentId: string;

  type:
    | "risk"
    | "tech_debt"
    | "security_concern"
    | "performance_concern"
    | "cost_concern"
    | "maintenance_reminder"
    | "proposal"
    | "blocked_capability";

  severity: "info" | "low" | "medium" | "high" | "critical";

  title: string;
  summary: string;
  evidence: string;
  recommendation: string;

  status:
    | "new"
    | "acknowledged"
    | "converted_to_ticket"
    | "dismissed"
    | "snoozed";

  suggestedTicketTitle?: string;
  suggestedTicketDescription?: string;
  suggestedAssigneeAgentId?: string;
  suggestedPriority?: string;

  createdAt: Date;
  updatedAt: Date;
}
```

### 15.3 Workspace Inbox UI

The UI should include a Workspace Inbox or Signals page.

Signal card shows:

```text
Title
Agent
Severity
Type
Evidence summary
Recommendation
Status
```

Actions:

```text
Create Ticket
Assign to Agent
Ask Follow-up
Acknowledge
Dismiss
Snooze
Update Agent Permission
Grant Capability
```

### 15.4 Observation Jobs

Agents can have observation jobs independent of tickets.

Job types:

```text
work_on_ticket
respond_to_mention
review_ticket
qa_ticket
observe_domain
inspect_health
create_signal
answer_owner_question
```

Example schedule:

```yaml
scheduled_observations:
  - agent: dba-agent
    schedule: "daily"
    task: "Inspect database health summary and raise concerns if needed."

  - agent: security-agent
    schedule: "weekly"
    task: "Review dependency/security reports and raise concerns if needed."
```

For v1, support manual `Run Observation` first. Scheduled observation can come later.

### 15.5 Evidence Requirement

Proactive signals must include evidence and recommendation.

Bad signal:

```text
DB may be unsafe. Improve it.
```

Good signal:

```text
Observed:
- DB CPU exceeded 85% during nightly jobs on 4 of last 7 days.
- Disk usage increased from 61% to 74% in 21 days.
- No replica configured.
- Last backup verification is unknown.

Risk:
Standalone DB failure can cause full outage.

Recommendation:
Create a ticket to evaluate read replica, backup verification, and slow query optimization.
```

### 15.6 Anti-Spam Rules

Proactive agents must not spam the owner.

Recommended policy:

```yaml
proactive_policy:
  enabled: true
  max_signals_per_agent_per_day: 3
  min_severity_to_notify_owner: medium
  auto_snooze_duplicates_days: 14
  require_evidence: true
  require_recommendation: true
```

Duplicate signals should update existing signals instead of creating new ones.

## 16. Agent Context Package

Before invoking an agent CLI, the system should generate a context package.

Example file:

```text
.agent/context.md
```

Recommended sections:

```text
Current task or observation goal
Agent role and responsibilities
Allowed capabilities
Available commands and tools
Available secrets by name only
Current ticket/signal context
Relevant comments
Relevant artifacts
Retrieved knowledge
Project rules
Sandbox limitations
Expected output contract
```

Agents should know what they can and cannot do.

Example instruction:

```text
If you need a command, secret, network host, or path that is not available, do not guess. Return a blocked result explaining the missing capability and mention the owner.
```

## 17. Agent Result Contract

Agents should return machine-readable results.

Implementation done:

```json
{
  "status": "done",
  "summary": "Implemented retry policy for payment polling.",
  "changedFiles": [
    "src/features/payment/usePaymentPolling.ts",
    "src/features/payment/usePaymentPolling.test.ts"
  ],
  "testsRun": [
    "pnpm test src/features/payment/usePaymentPolling.test.ts"
  ],
  "nextStatus": "In Review",
  "mentionAgents": ["tech-lead-agent"],
  "blockers": []
}
```

Blocked by missing capability:

```json
{
  "status": "blocked",
  "blockerType": "missing_capability",
  "summary": "Cannot inspect database because psql and DB_READONLY_URL are unavailable.",
  "requiredCapabilities": ["postgres_readonly_inspection"],
  "requiredCommands": ["psql"],
  "requiredSecrets": ["DB_READONLY_URL"],
  "nextStatus": "Waiting for Owner",
  "mentionAgents": ["owner"]
}
```

Proactive signal:

```json
{
  "status": "signal_created",
  "signal": {
    "type": "risk",
    "severity": "high",
    "title": "Standalone PostgreSQL instance is becoming an operational risk",
    "summary": "Database load and storage growth indicate the current standalone setup should be reviewed.",
    "evidence": "CPU exceeded 85% on 4 of last 7 nightly jobs; disk usage grew from 61% to 74% in 21 days; no replica detected.",
    "recommendation": "Create a ticket to evaluate read replica, backup verification, monitoring, and slow query optimization.",
    "suggestedTicketTitle": "Evaluate PostgreSQL HA and backup strategy"
  }
}
```

## 18. Permissions and Safety Defaults

### 18.1 Default Permissions by Role

PM Agent:

```text
Read tickets/comments
Create/refine tickets
Ask clarification
No repo write
No secrets by default
```

Tech Lead Agent:

```text
Read repo
Review diffs
Create technical plans
Comment review feedback
No production secrets by default
No push by default
```

Engineer Agent:

```text
Read/write worktree
Run allowed build/test commands
Commit locally
No push by default
No production secrets by default
```

QC Agent:

```text
Run app/test commands
Attach screenshots
Report bugs
No code write by default unless configured
```

DBA Agent:

```text
Readonly DB credentials only
Allowed database inspection commands
No destructive DB commands
No schema change execution by default
```

Security Agent:

```text
Read dependency reports
Run allowed scan commands
Raise concerns
No secret exfiltration
No production changes
```

### 18.2 Sensitive Actions Require Human Approval

Default approval gates:

```text
Granting secrets
Granting new capabilities
Allowing network access
Installing dependencies
Changing lockfiles
Running migrations
Pushing branches
Creating PRs if configured as sensitive
Merging PRs
Production deployment
Destructive commands
```

## 19. UI Overview

### 19.1 Main Screens

Recommended screens:

```text
Board
Ticket Detail
Agents
Workspace Inbox / Signals
Knowledge
Capabilities
Sandbox Profiles
Secrets
Settings
```

The UI should be simple, fast, and mostly API-driven.

### 19.2 Board Screen

Trello-like columns. Drag cards to update status. Click card to open detail.

### 19.3 Ticket Detail Screen

Primary collaborative workspace for each task.

Tabs:

```text
Description
Comments
Live Console
Artifacts
Runs
Knowledge Used
Metadata
```

### 19.4 Agent Management Screen

Agent detail tabs:

```text
Profile
Role & Prompt
Provider
Capabilities
Sandbox
Secrets
Observation Schedule
Memory
```

### 19.5 Workspace Inbox / Signals Screen

Shows proactive agent messages and capability blockers.

Actions:

```text
Create Ticket
Ask Follow-up
Grant Capability
Add Secret
Snooze
Dismiss
Acknowledge
```

### 19.6 Knowledge Screen

Shows pending/approved/rejected/stale knowledge.

Actions:

```text
Approve
Edit
Reject
Supersede
Mark stale
View source ticket/signal
```

### 19.7 Capabilities, Sandbox, and Secrets Screens

These screens should be simple configuration pages, not complex enterprise RBAC.

Capabilities show high-level access bundles.
Sandbox profiles show command/path/network/resource limits.
Secrets show names, scopes, allowed agents/profiles, and rotation metadata, but never raw secret values after creation.

## 20. Server Responsibilities

The server contains all core logic. The UI is only a client using HTTP API and WebSocket.

Server responsibilities:

```text
Ticket state
Workflow transitions
Agent configuration
Job queue
Agent runs
Live session streaming
Worktree management
Sandbox enforcement
Capability resolution
Secret injection
Comment/mention handling
Workspace signals
Knowledge retrieval
Learning extraction
Artifact management
Audit logging
```

The frontend should not implement workflow rules.

## 21. Suggested Technical Direction

Although this document is product-focused, the intended architecture is:

```text
Backend: Rust server
Database: PostgreSQL + pgvector
Frontend: lightweight React SPA
Communication: HTTP API + WebSocket
Agent execution: CLI providers inside sandbox sessions
Artifacts: filesystem first, object storage later
Deployment: Docker Compose
```

The server should be optimized for low memory, safety, predictable process management, and long-running reliability.

## 22. API Surface Overview

Example API groups:

```text
/projects
/repos
/tickets
/tickets/:id/comments
/tickets/:id/assign
/tickets/:id/run-agent
/tickets/:id/move-status
/agents
/agent-runs
/agent-runs/:id/stop
/agent-runs/:id/retry
/knowledge
/signals
/capabilities
/sandbox-profiles
/secrets
/settings
```

WebSocket endpoints:

```text
/ws/events
/ws/agent-runs/:id/live
```

Event examples:

```text
ticket.updated
comment.created
agent.mentioned
agent_run.started
agent_run.finished
agent_run.blocked
signal.created
blocker.created
knowledge.proposed
```

## 23. Data Storage Overview

Use PostgreSQL for structured data:

```text
projects
repos
agents
agent_presets
tickets
ticket_comments
ticket_mentions
attachments metadata
agent_jobs
agent_runs
workflow_rules
knowledge_items
knowledge_embeddings
knowledge_usage_logs
workspace_signals
capabilities
sandbox_profiles
secrets metadata
blockers
```

Use filesystem/object storage for heavy data:

```text
terminal logs
screenshots
uploaded files
diff artifacts
test reports
review reports
repo clones
worktrees
```

## 24. Build Phases

### Phase 1: Core Board and Agents

```text
Projects/repos
Tickets
Comments
Agents CRUD
Simple board
Manual assignment
PostgreSQL schema
```

### Phase 2: Knowledge v1

```text
Knowledge items
pgvector embeddings
Manual project rules
Basic retrieval
Context builder includes knowledge
```

### Phase 3: Agent Runner

```text
CLI provider adapter
Worktree creation
Agent run lifecycle
Result contract
Run logs
Agent comments
```

### Phase 4: Live Console

```text
tmux/PTY session per run
WebSocket terminal stream
Stop/retry controls
Saved terminal logs
```

### Phase 5: Comments, Mentions, and Workflow

```text
Mention creates jobs
Clarification flow
Waiting statuses
Resume jobs
Review/QA/final-human flow
```

### Phase 6: Capabilities, Sandbox, and Secrets

```text
Capability model
Sandbox profiles
Secret metadata and injection
Missing capability blocker flow
Guided unblock UI
```

### Phase 7: Proactive Signals

```text
Workspace Inbox
Manual observation jobs
Agent-created signals
Convert signal to ticket
Signal deduplication and snooze
```

### Phase 8: Learning Extractor and Memory Hygiene

```text
Candidate knowledge extraction
Knowledge approval inbox
Usage logging
Expiry/supersede
Consolidation job
```

### Phase 9: PR and External Integrations

```text
Push branch
Create PR
GitHub/GitLab integration
CI status
Optional scheduled observation jobs
```

## 25. Non-Goals for Early Versions

Do not build these early:

```text
Complex Jira-like workflow builder
Enterprise RBAC
Multi-tenant SaaS
Agent marketplace
Heavy analytics dashboard
Autonomous production deployment
Automatic destructive database changes
Full browser automation farm
Kubernetes runner
Parallel swarm execution
```

The first versions should stay focused on:

```text
Simple board
Agent assignment
Ticket comments
Live runs
Knowledge retrieval
Sandboxed capabilities
Proactive signals
Human final review
```

## 26. Example End-to-End Scenario

### Scenario: DBA Agent Raises a Proactive Risk

```text
1. DBA Agent has the postgres_readonly_inspection capability.

2. Owner manually runs "Observe DB Health" or the scheduled observation starts.

3. Server resolves DBA Agent sandbox:
   - command: psql
   - secret: DB_READONLY_URL
   - network: postgres.internal
   - readonly profile

4. DBA Agent runs inside sandbox and inspects allowed DB health views.

5. DBA Agent notices:
   - DB is standalone
   - disk usage is growing quickly
   - slow query count increased
   - no replica detected

6. DBA Agent creates Workspace Signal:
   "Standalone PostgreSQL instance is becoming an operational risk."

7. Owner opens Workspace Inbox.

8. Owner clicks "Create Ticket".

9. Ticket is created:
   "Evaluate PostgreSQL HA and backup strategy."

10. Ticket is assigned to Tech Lead Agent.

11. Tech Lead Agent asks DBA Agent for more evidence through a ticket comment.

12. DBA Agent responds with metrics.

13. Tech Lead Agent prepares a plan.

14. Human final review decides whether to execute the plan.
```

### Scenario: Engineer Blocked by Missing Capability

```text
1. FE Agent receives a ticket.

2. FE Agent starts work in a sandboxed worktree.

3. It needs to run pnpm test, but pnpm is not allowed in the sandbox profile.

4. FE Agent returns blocked result:
   "Need command pnpm to run project tests."

5. Ticket moves to Waiting for Owner.

6. UI shows guided unblock:
   [Allow pnpm]
   [Reject]
   [Ask agent why]

7. Owner grants capability.

8. System resumes FE Agent.
```

## 27. Final Positioning

The final product should be positioned as:

```text
A self-hosted agent workspace where AI teammates do assigned work, monitor their owned domains, ask for missing access, and raise proactive engineering concerns inside strict sandboxes.
```

Shorter version:

```text
Trello for AI engineering agents, with live terminals, controlled memory, sandboxed capabilities, and proactive role ownership.
```

The product should not promise fully autonomous software engineering. It should promise controlled autonomy: agents can move work forward, communicate clearly, learn safely, and raise risks, while humans keep final authority over sensitive decisions.
