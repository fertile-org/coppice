import { CollapsibleDetail, excerptPreview } from '../../opencode-session/components/CollapsibleDetail';
import { MarkdownContent } from '../../opencode-session/components/MarkdownContent';
import { PlainOutput } from '../../opencode-session/components/PlainOutput';
import { AgentResultCard } from '../../opencode-session/parts/AgentResultCard';
import { ToolOutput, ToolShell } from '../../opencode-session/tools/ToolShell';
import { sessionTheme } from '../../opencode-session/theme/session-theme';
import type { ClaudeConsoleEntry } from './claude-console-state';

function SessionLine({ model }: { model: string }) {
  return (
    <p className={`${sessionTheme.fontMonoSm} ${sessionTheme.textMuted}`}>
      → Claude session started (model: {model})
    </p>
  );
}

function ThinkingEntry({
  text,
  streaming,
}: {
  text: string;
  streaming?: boolean;
}) {
  const content = text.replaceAll('[REDACTED]', '').trim();
  if (!content) return null;

  return (
    <CollapsibleDetail
      label="Thinking"
      preview={excerptPreview(content)}
      streaming={streaming}
    >
      <div className={sessionTheme.fontMonoSm}>
        <MarkdownContent tone="thinking">{content}</MarkdownContent>
      </div>
    </CollapsibleDetail>
  );
}

function TextEntry({ markdown }: { markdown: string }) {
  const content = markdown.trim();
  if (!content) return null;
  return (
    <div>
      <MarkdownContent>{content}</MarkdownContent>
    </div>
  );
}

function ToolEntry({
  entry,
}: {
  entry: Extract<ClaudeConsoleEntry, { kind: 'tool' }>;
}) {
  return (
    <ToolShell
      variant={entry.variant}
      status={entry.status}
      title={entry.title}
    >
      {entry.output ? (
        <ToolOutput>
          <PlainOutput
            text={entry.output}
            language={entry.variant === 'shell' ? 'bash' : undefined}
          />
        </ToolOutput>
      ) : null}
    </ToolShell>
  );
}

function ContinuedEntry({
  summary,
  progressNote,
}: {
  summary: string;
  progressNote?: string;
}) {
  return (
    <div className={`overflow-hidden border border-[var(--oc-border)] ${sessionTheme.bgElement}`}>
      <div className="border-b border-[var(--oc-border)] px-3 py-2">
        <span className={`${sessionTheme.fontMonoSm} font-medium ${sessionTheme.info}`}>
          Continued
        </span>
      </div>
      <div className="px-3 py-3">
        <MarkdownContent>{summary}</MarkdownContent>
        {progressNote?.trim() ? (
          <p className={`mt-3 ${sessionTheme.fontMonoSm} ${sessionTheme.textMuted}`}>
            Progress:{' '}
            <span className={sessionTheme.text}>{progressNote}</span>
          </p>
        ) : null}
      </div>
    </div>
  );
}

function renderEntry(
  entry: ClaudeConsoleEntry,
  index: number,
  total: number,
  isLive: boolean,
) {
  switch (entry.kind) {
    case 'session':
      return <SessionLine key={entry.id} model={entry.model} />;
    case 'thinking':
      return (
        <ThinkingEntry
          key={entry.id}
          text={entry.text}
          streaming={isLive && index === total - 1}
        />
      );
    case 'text':
      return <TextEntry key={entry.id} markdown={entry.markdown} />;
    case 'tool':
      return <ToolEntry key={entry.id} entry={entry} />;
    case 'result':
      return <AgentResultCard key={entry.id} contract={entry.contract} />;
    case 'continued':
      return (
        <ContinuedEntry
          key={entry.id}
          summary={entry.summary}
          progressNote={entry.progressNote}
        />
      );
    default:
      return null;
  }
}

export function ClaudeConsoleView({
  entries,
  legacyText,
  isLive = false,
}: {
  entries: ClaudeConsoleEntry[];
  legacyText: string;
  isLive?: boolean;
}) {
  const hasStructured = entries.length > 0;
  const legacy = legacyText.trim();

  if (!hasStructured && !legacy) {
    return (
      <p className={`${sessionTheme.fontBody} ${sessionTheme.textMuted}`}>
        Waiting for agent output…
      </p>
    );
  }

  return (
    <div className={`flex flex-col ${sessionTheme.sectionGap}`}>
      {entries.map((entry, index) =>
        renderEntry(entry, index, entries.length, isLive),
      )}
      {legacy ? (
        <pre
          className={`overflow-x-auto whitespace-pre-wrap ${sessionTheme.fontMonoSm} ${sessionTheme.textMuted}`}
        >
          {legacy}
        </pre>
      ) : null}
    </div>
  );
}
