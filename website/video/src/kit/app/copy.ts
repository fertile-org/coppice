// Strings copied verbatim from the Coppice app (see README "Verified strings").
// Posts take labels from here so nothing on screen drifts from the real app.

/** Board columns in app order. web/src/features/board/columns.ts (BOARD_COLUMNS). */
export const COLUMNS = [
  { name: "Backlog", colorKey: "backlog" },
  { name: "Plan Review", colorKey: "plan-review" },
  { name: "Ready", colorKey: "ready" },
  { name: "In Progress", colorKey: "in-progress" },
  { name: "In Review", colorKey: "in-review" },
  { name: "In QA", colorKey: "in-qa" },
  { name: "Wait for Human Review", colorKey: "wait-final" },
  { name: "Done", colorKey: "done" },
] as const;

export type ColumnName = (typeof COLUMNS)[number]["name"];
export type ColumnColorKey = (typeof COLUMNS)[number]["colorKey"];

/** web/src/features/plan/copy.ts (PLAN_COPY). */
export const PLAN = {
  approvePlan: "Approve plan",
  askForChanges: "Ask for changes",
  askPlaceholder: "What should change?",
  skipHumanReview: "Skip human review",
  skipPlanning: "Skip planning",
  skipPlanningHelp: "Goes straight from Backlog to Ready without a plan.",
  proposedPlan: "Proposed plan",
  revision: (n: number) => `Revision ${n}`,
  approveAddsToDescription:
    "Approving adds this plan to the description, under Approved plan.",
  // Card plan-state lines (TicketCard via planCardState).
  writingPlan: "Writing plan",
  waitingForApproval: "Waiting for you to approve the plan",
  waitingForAnswers: "Waiting for your answers",
  planNeedsRun: "Plan needs a run",
  runAgent: "Run agent",
  approving: "Approving…",
  requestedChanges: "Requested changes",
} as const;

/** web/src/features/designs/designCopy.ts (DESIGN_COPY). */
export const DESIGN = {
  cardTitle: "Click to open design options",
  open: "Open",
  dialogTitle: "Design options",
  recommended: "Recommended",
  chosen: "Chosen",
  options: (n: number) => `${n} ${n === 1 ? "option" : "options"}`,
  optionName: (option: { label: string; title: string }) => `${option.label}: ${option.title}`,
  using: (option: { label: string; title: string }, yourChoice: boolean) =>
    `Using ${option.label}: ${option.title} (${yourChoice ? "your choice" : "recommended"})`,
  workFollows: "Work follows this option.",
  planRunning: "The plan is being updated.",
  chooseAndUpdate: (label: string) => `Choose ${label} and update plan`,
} as const;

/** Server: server/src/copy/plan.rs (plan_design_change_note), the request a design pick files. */
export const designChangeNote = (option: { label: string; title: string }) =>
  `Update the plan to use design option ${option.label}: ${option.title}.`;

/**
 * The design set the app's own screenshots publish for "Add a dark mode toggle to Settings"
 * (e2e/screenshot/design-options.mjs: DESIGN_SET_TITLE, DESIGN_OPTIONS; RECOMMENDED_OPTION_ID
 * is option A). Shown in static/screenshots/08-design-options.png. The preview strings are
 * the option pages' own text from the same file.
 */
export const DARK_MODE_DESIGNS = {
  setTitle: "Dark mode toggle on Settings",
  recommended: "A",
  options: [
    { label: "A", title: "Switch in the Settings row" },
    { label: "B", title: "Light, Dark and System buttons" },
    { label: "C", title: "Theme preview cards" },
  ],
  preview: {
    heading: "Settings",
    lede: "Preferences for acme-web on this machine.",
    darkMode: { name: "Dark mode", hint: "Use the dark theme on this machine." },
    theme: { name: "Theme", hint: "System follows your computer's setting." },
    themeCards: { name: "Theme", hint: "Pick the look you want to work in." },
    segments: ["Light", "Dark", "System"],
    cards: ["Light", "Dark"],
    language: { name: "Language", hint: "Used for dates and labels.", value: "English" },
    startPage: { name: "Start page", hint: "What opens first.", value: "Dashboard" },
  },
} as const;

export type DesignOptionLabel = (typeof DARK_MODE_DESIGNS.options)[number]["label"];

/** Server: server/src/copy/plan.rs (APPROVED_PLAN_SECTION, PLAN_HEADING). */
export const PLAN_MARKDOWN = {
  planHeading: "## Plan",
  approvedPlanSection: "## Approved plan",
} as const;

/** web/src/features/plan/copy.ts (PLAN_COPY.approvedPlan): the code review page's plan column. */
export const APPROVED_PLAN = "Approved plan";

/**
 * The code review page, web/src/features/code/: CodeReviewPage.tsx (header, view toggle),
 * CodeReviewBackButton.tsx, ChangedFilesPanel.tsx, FileReviewHeader.tsx, DiffViewer.tsx
 * (InlineCommentWidget, PendingCommentBubble).
 */
export const CODE_REVIEW = {
  back: "Back",
  codeReview: "Code review",
  into: "into",
  worktree: "Worktree",
  base: "Base",
  split: "Split",
  unified: "Unified",
  hideWhitespace: "Hide whitespace",
  submitReview: "Submit review",
  changedFiles: "Changed files",
  viewedProgress: (viewed: number, total: number) => `${viewed} of ${total} viewed`,
  viewed: "Viewed",
  prev: "‹ Prev",
  prevKey: "K",
  next: "Next",
  nextKey: "J",
  clickLineNumber: "Click a line number to add an inline comment.",
  leaveComment: "Leave a comment…",
  addComment: "Add comment",
  cancel: "Cancel",
  edit: "Edit",
  remove: "Remove",
} as const;

/** CodeReviewPage.tsx (worktreeLabel): the Worktree select's label. */
export const worktreeLabel = (path: string, branch: string, ticketTitle?: string) => {
  const shortPath = path.split("/").pop() ?? path;
  return ticketTitle ? `${shortPath} · ${branch} · ${ticketTitle}` : `${shortPath} · ${branch}`;
};

/** web/src/features/code/SubmitReviewDialog.tsx; "Review posted" is its success toast. */
export const SUBMIT_REVIEW = {
  title: "Submit review",
  descriptionLinked: "Post a combined review comment on the linked ticket.",
  summary: "Summary",
  summaryPlaceholder: "Overall feedback for this change…",
  workflowAction: "Workflow action",
  commentOnly: "Comment only",
  moveToInProgress: "Move to In Progress",
  reassignEngineer: "Reassign engineer",
  engineer: "Engineer",
  engineerOption: (name: string, role: string) => `${name} · ${role}`,
  preview: "Preview",
  cancel: "Cancel",
  submit: "Submit review",
  summaryRequired: "(summary required)",
  reviewPosted: "Review posted",
} as const;

export type WorkflowActionLabel =
  | typeof SUBMIT_REVIEW.commentOnly
  | typeof SUBMIT_REVIEW.moveToInProgress
  | typeof SUBMIT_REVIEW.reassignEngineer;

export type InlineComment = { path: string; line: number; side: "new" | "old" | "delete"; body: string };

/** web/src/features/code/formatReviewPreview.ts, verbatim: the dialog's Preview and the posted comment. */
export function formatReviewPreview(
  repoName: string,
  worktreePath: string,
  baseBranch: string,
  headBranch: string,
  headSha: string,
  summary: string,
  inlineComments: InlineComment[],
): string {
  let body = `## Code review\n\n**Repo:** ${repoName} · **Worktree:** ${worktreePath} · **Compare:** ${baseBranch} → ${headBranch} (${headSha})\n\n### Summary\n${summary}\n`;
  if (inlineComments.length === 0) return body;
  body += "\n### Inline comments\n";
  const byPath = new Map<string, InlineComment[]>();
  for (const comment of inlineComments) {
    byPath.set(comment.path, [...(byPath.get(comment.path) ?? []), comment]);
  }
  for (const path of [...byPath.keys()].sort()) {
    body += `\n#### \`${path}\`\n`;
    for (const comment of byPath.get(path)!) {
      body += `- **L${comment.line}** (${comment.side}): ${comment.body.trim()}\n`;
    }
  }
  return body;
}

/** web/src/features/tickets/TicketDrawer.tsx: the header button shown while a run is active. */
export const DRAWER = { stop: "Stop" } as const;

/** Server: the agent preset role (server/migrations/002_workspace.sql, frontend_engineer). */
export const FRONTEND_ENGINEER = { name: "Frontend", role: "Frontend Engineer" } as const;

/** A highlight.js class the app's diff colours (web/src/index.css, .code-review-diff .hljs-*). */
export type CodeToken = { text: string; kind?: "keyword" | "string" | "title" | "attr" };

/**
 * The diff the app's own screenshots review (static/screenshots/05-diff.png):
 * e2e/screenshot/seed-hero-repo.sh (repo acme-web, ThemeToggle.tsx on the ticket branch,
 * base has only SettingsHeading), capture-hero.mjs (TICKET_TITLE, LINE_COMMENT, the plan's
 * steps). Tokens are what the app's highlight.js (typescript) makes of the new file.
 * Ticket short id and commit prefixes are the ones in 05-diff.png; the rest of each sha is made up.
 */
export const THEME_TOGGLE_REVIEW = {
  repoName: "acme-web",
  ticketTitle: "Add a dark mode toggle to Settings",
  worktreePath: "/data/worktrees/TICKET-0ac6e090-acme-web",
  headBranch: "agent/TICKET-0ac6e090",
  headSha: "d68d78041c2b9e7a5f3d0c8e6b1a4f92e7d05c31",
  baseBranch: "main",
  baseSha: "602b3135e9a7c4d1b08f6e2a93c7d5b14f8e0a26",
  path: "src/settings/ThemeToggle.tsx",
  status: "M",
  additions: 15,
  deletions: 0,
  planSteps: ["Add the Settings toggle", "Use the existing theme hook", "Add a settings test"],
  comment: "Use the existing theme hook.",
  /** Unified diff rows: old line number (absent on inserts), new line number, tokens. */
  rows: [
    { newLine: 1, insert: true, tokens: [{ text: "import", kind: "keyword" }, { text: " { useState } " }, { text: "from", kind: "keyword" }, { text: " " }, { text: "'react'", kind: "string" }, { text: ";" }] },
    { newLine: 2, insert: true, tokens: [] },
    { oldLine: 1, newLine: 3, tokens: [{ text: "export", kind: "keyword" }, { text: " " }, { text: "function", kind: "keyword" }, { text: " " }, { text: "SettingsHeading", kind: "title" }, { text: "() {" }] },
    { oldLine: 2, newLine: 4, tokens: [{ text: "  " }, { text: "return", kind: "keyword" }, { text: " <" }, { text: "h1", kind: "title" }, { text: ">Settings</" }, { text: "h1", kind: "title" }, { text: ">;" }] },
    { oldLine: 3, newLine: 5, tokens: [{ text: "}" }] },
    { newLine: 6, insert: true, tokens: [] },
    { newLine: 7, insert: true, tokens: [{ text: "export", kind: "keyword" }, { text: " " }, { text: "function", kind: "keyword" }, { text: " " }, { text: "ThemeToggle", kind: "title" }, { text: "() {" }] },
    { newLine: 8, insert: true, tokens: [{ text: "  " }, { text: "const", kind: "keyword" }, { text: " [theme, setTheme] = " }, { text: "useState", kind: "title" }, { text: "(" }, { text: "'light'", kind: "string" }, { text: ");" }] },
    { newLine: 9, insert: true, tokens: [] },
    { newLine: 10, insert: true, tokens: [{ text: "  " }, { text: "return", kind: "keyword" }, { text: " (" }] },
    { newLine: 11, insert: true, tokens: [{ text: "    <" }, { text: "button", kind: "title" }] },
    { newLine: 12, insert: true, tokens: [{ text: "      " }, { text: "type", kind: "attr" }, { text: "=" }, { text: '"button"', kind: "string" }] },
    { newLine: 13, insert: true, tokens: [{ text: "      " }, { text: "onClick", kind: "attr" }, { text: "=" }, { text: "{()", kind: "string" }, { text: " => setTheme(theme === 'dark' ? 'light' : 'dark')}" }] },
    { newLine: 14, insert: true, tokens: [{ text: "    >" }] },
    { newLine: 15, insert: true, tokens: [{ text: "      {theme === 'dark' ? 'Use light mode' : 'Use dark mode'}" }] },
    { newLine: 16, insert: true, tokens: [{ text: "    </" }, { text: "button", kind: "title" }, { text: ">" }] },
    { newLine: 17, insert: true, tokens: [{ text: "  );" }] },
    { newLine: 18, insert: true, tokens: [{ text: "}" }] },
  ] as ReadonlyArray<{ oldLine?: number; newLine: number; insert?: boolean; tokens: CodeToken[] }>,
} as const;

/**
 * Agents page: web/src/features/agents/AgentsPage.tsx (page header, table headers, AgentRow
 * pills and buttons, EditAgentDialog title and description), AgentForm.tsx (field labels,
 * footer buttons), RefreshModelsButton.tsx, copy.ts (AGENT_COPY), the plugins copy
 * (web/src/features/plugins/copy.ts, PLUGIN_COPY.nextRun).
 */
export const AGENTS = {
  title: "Agents",
  subtitle: "Configure your agent team from presets.",
  newAgent: "New agent",
  columns: ["Name", "Role", "Connector", "Status", "Health", "Updated"],
  enabled: "Enabled",
  healthy: "Healthy",
  disable: "Disable",
  edit: "Edit",
  /** AgentsPage.tsx agentModelLine: `{provider}/{model}`. */
  modelLine: (provider: string, model: string) => `${provider}/${model}`,
  editTitle: "Edit agent",
  editDescription: (name: string) => `Update ${name}'s configuration.`,
  name: "Name",
  role: "Role",
  skills: "Skills",
  skillsHint: "(plugins this agent can use)",
  noPlugins: ["No enabled plugins. Enable plugins on the", "Plugins", "page."],
  pluginsNextRun: "Changes apply from each agent's next run.",
  responsibilities: "Responsibilities",
  onePerLine: "(one per line)",
  systemPrompt: "System prompt",
  connector: "Connector",
  model: "Model",
  refreshModels: "Refresh models",
  thinkingLevel: "Thinking level",
  cancel: "Cancel",
  saveChanges: "Save changes",
  saving: "Saving…",
  searchConnectors: "Search connectors…",
} as const;

/**
 * web/src/features/agents/KnowledgeCompactionCard.tsx, admin view with compaction off; the status
 * line from formatInterval / pluralTickets (web/src/features/knowledge/compactionFormat.ts) with the
 * server defaults interval_secs 1800 and batch_max_tickets 10 (config/src/lib.rs).
 */
export const KNOWLEDGE_COMPACTION = {
  title: "Knowledge compaction",
  description:
    "Agent that turns Done tickets into knowledge candidates for review. Leave empty to turn compaction off.",
  agentLabel: "Compaction agent",
  none: "None — compaction off",
  save: "Save",
  status: "Runs every 30 min in batches of up to 10 tickets, read-only, no repo. 0 Done tickets are waiting.",
} as const;

/**
 * Connectors as the agent form lists them (AgentForm.tsx connectorPickerOptions, connectorOptionLabel;
 * web/src/features/tools/connectorCopy.ts readinessLabel): connectors that are on come first, the rest
 * keep their status. Ids, names and order from connectors/src/lib.rs (Mock is listed only in builds
 * that register it). Here Claude Code, Codex and OpenCode are installed and signed in (on); the others
 * are not installed ("Not on your PATH").
 */
export const CONNECTORS = {
  claudeCode: { id: "claude-code", name: "Claude Code" },
  codex: { id: "codex", name: "Codex" },
  opencode: { id: "opencode", name: "OpenCode" },
  ready: "Ready",
  notOnPath: "Not on your PATH",
  others: ["Cursor", "Kilo Code", "GitHub Copilot", "Qwen Code", "Kimi Code", "Grok"],
  optionLabel: (name: string, status: string) => `${name} — ${status}`,
  /** connectorCopy.ts connectorDefaultModelLabel: the first model option. */
  defaultModel: (name: string) => `${name}'s default`,
} as const;

/**
 * Models. Claude Code: server/src/providers/models.rs (curated list; providers sonnet, opus, haiku, fable
 * from connectors/src/lib.rs default_model_providers).
 */
export const MODELS = {
  opus: { provider: "opus", id: "claude-opus-5-5", name: "Claude Opus 5.5" },
  sonnet: { provider: "sonnet", id: "claude-sonnet-5-5", name: "Claude Sonnet 5.5" },
  haiku: { provider: "haiku", id: "claude-haiku-5-5", name: "Claude Haiku 5.5" },
} as const;

/**
 * OpenCode's models, live from `opencode models` (server/src/providers/opencode_models.rs): each line is
 * `<provider>/<model>`; the providers are discovered from the same output (models.rs OpenCodeModels,
 * cli_catalog.rs discover_cli_providers); the id is the line without `<provider>/`, the name equals the id,
 * sorted by id. One provider here (openrouter), so the list has no group headings. Example content: the
 * model ids are what an OpenRouter account would list; Qwen 3.8 is the owner's pick.
 */
export const OPENCODE_MODELS = {
  provider: "openrouter",
  /** Sorted as parse_opencode_models_stdout sorts them. */
  ids: ["moonshotai/kimi-k3", "qwen/qwen3.8", "z-ai/glm-5.1"],
  qwen: { provider: "openrouter", id: "qwen/qwen3.8", name: "qwen/qwen3.8" },
} as const;

export type ModelChoice = (typeof MODELS)[keyof typeof MODELS];

/**
 * The agents the app's own hero screenshots seed (e2e/screenshot/capture-hero.mjs seedAgents: PM, Frontend,
 * QA on claude-code), with the preset keys and roles from server/migrations/002_workspace.sql. The list is
 * ordered by PRESET_ORDER (server/src/domain/agent.rs).
 */
export const HERO_AGENTS = {
  pm: { name: "PM", presetSource: "pm", role: "PM" },
  frontend: { name: FRONTEND_ENGINEER.name, presetSource: "frontend_engineer", role: FRONTEND_ENGINEER.role },
  qa: { name: "QA", presetSource: "qc", role: "QC" },
} as const;

/**
 * The frontend_engineer preset's responsibilities after migrations 008 and 037
 * (server/migrations/008_agent_template_files.sql, 037_fold_agent_skills_into_responsibilities.sql),
 * and the start of its system prompt: server/agent_templates/frontend_engineer.md, whose first line
 * `{{posture}}` is replaced by server/agent_templates/posture.md.
 */
export const FRONTEND_PRESET = {
  responsibilities: [
    "implement frontend tickets",
    "follow project UI conventions",
    "fix UI defects",
    "raise frontend tech debt",
    "UI implementation",
    "component design",
    "accessibility",
    "frontend testing",
  ],
  systemPromptHead: [
    "# Operating posture",
    "",
    "## Stance",
    "Be direct, practical, opinionated, and high-agency.",
    "Useful beats agreeable. Push back on vague tickets, unrealistic scope, or avoidable risk — with evidence (code, tests, docs, tradeoffs), not sport.",
    "Separate facts, assumptions, judgment calls, and open questions. Say what matters and stop.",
    "Create motion on the assigned ticket: if blocked, state what is missing, what you tried, and what should happen next.",
    "Prefer the worktree, ticket, and repo docs over guessing; state unknowns instead of inventing APIs or rules.",
    "",
    "## Hard never (without explicit human approval)",
    "- posting publicly or publishing externally",
    "- purchasing anything or signing up for paid services",
    "- sending messages to real people outside the workspace",
    "- deleting important work or making destructive, irreversible changes",
    "- exposing private information, secrets, or credentials",
    "- changing credentials, permissions, or security settings",
    "- pushing to remote or merging without a human gate when the project requires it",
  ],
} as const;
