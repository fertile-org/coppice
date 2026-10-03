import { ChevronDown, ChevronRight } from 'lucide-react';
import { useState, type ReactNode } from 'react';
import { Link } from 'react-router-dom';

const GUIDE_OPEN_KEY = 'coppice.plugins.guideOpen';

const PARTS: { term: string; detail: string }[] = [
  {
    term: 'Skills',
    detail:
      'Markdown instructions. Agents see each skill’s name and description, and load the full text only when they need it.',
  },
  {
    term: 'Tools',
    detail: 'MCP servers the plugin starts or connects to. Agents call their tools during runs.',
  },
  {
    term: 'Settings',
    detail: 'Values a tool needs, such as API keys. Stored encrypted and never shown again.',
  },
];

function readStoredOpen(): boolean | null {
  try {
    const value = localStorage.getItem(GUIDE_OPEN_KEY);
    return value === null ? null : value === '1';
  } catch {
    return null;
  }
}

function Step({ n, title, children }: { n: number; title: string; children: ReactNode }) {
  return (
    <li className="flex gap-3">
      <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-accent font-body text-xs font-semibold text-accent-foreground">
        {n}
      </span>
      <p className="font-body text-sm text-text-secondary">
        <span className="font-medium text-text-primary">{title}</span> — {children}
      </p>
    </li>
  );
}

/** Open by default until the workspace has plugins; the admin's toggle is remembered. */
export function PluginsGuide({ hasPlugins }: { hasPlugins: boolean }) {
  const [stored, setStored] = useState(readStoredOpen);
  const open = stored ?? !hasPlugins;

  function toggle() {
    const next = !open;
    setStored(next);
    try {
      localStorage.setItem(GUIDE_OPEN_KEY, next ? '1' : '0');
    } catch {
      // Storage unavailable: the choice lasts until the page is left.
    }
  }

  return (
    <section className="rounded-xl border border-border bg-surface-raised shadow-card">
      <button
        type="button"
        aria-expanded={open}
        onClick={toggle}
        className="flex w-full items-center gap-2 px-4 py-3 text-left font-body text-sm font-medium text-text-primary"
      >
        {open ? (
          <ChevronDown className="h-4 w-4" aria-hidden />
        ) : (
          <ChevronRight className="h-4 w-4" aria-hidden />
        )}
        How plugins work
      </button>
      {open && (
        <div
          data-testid="plugins-guide"
          className="grid gap-6 border-t border-border px-4 py-4 md:grid-cols-2"
        >
          <div>
            <p className="font-body text-sm text-text-secondary">A plugin can give agents:</p>
            <dl className="mt-2 space-y-2">
              {PARTS.map((part) => (
                <div key={part.term}>
                  <dt className="font-body text-sm font-medium text-text-primary">{part.term}</dt>
                  <dd className="font-body text-sm text-text-secondary">{part.detail}</dd>
                </div>
              ))}
            </dl>
          </div>
          <div>
            <p className="font-body text-sm text-text-secondary">To use one:</p>
            <ol className="mt-2 space-y-2">
              <Step n={1} title="Add">
                install it from git or add a folder that contains it.
              </Step>
              <Step n={2} title="Enable">
                plugins start disabled. Fill in any settings, then press Test.
              </Step>
              <Step n={3} title="Attach">
                select it on an agent in{' '}
                <Link
                  to="/agents"
                  className="font-medium text-moss-700 underline-offset-2 hover:underline"
                >
                  Agents
                </Link>
                . Only attached agents get it.
              </Step>
            </ol>
          </div>
          <p className="font-body text-xs text-text-muted md:col-span-2">
            Plugins use the Claude Code plugin format (
            <code className="font-mono">.claude-plugin/plugin.json</code>,{' '}
            <code className="font-mono">skills/</code>, <code className="font-mono">.mcp.json</code>
            ). <code className="font-mono">commands/</code>, <code className="font-mono">agents/</code>{' '}
            and <code className="font-mono">hooks/</code> are not supported yet. To write your own,
            see <code className="font-mono">docs/plugins.md</code>.
          </p>
        </div>
      )}
    </section>
  );
}
