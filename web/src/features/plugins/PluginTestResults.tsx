import type { PluginTestResult, PluginTestServer } from '../../lib/schemas/plugin';

function statusClass(status: PluginTestServer['status']): string {
  const base = 'rounded-full border px-2 py-0.5 font-body text-xs';
  switch (status) {
    case 'ok':
      return `${base} border-success-muted bg-success-muted text-success`;
    case 'error':
      return `${base} border-danger-muted bg-danger-muted/40 text-danger`;
    case 'unsupported':
      return `${base} border-warning-muted bg-warning-muted text-warning`;
  }
}

export function PluginTestResults({ result }: { result: PluginTestResult }) {
  return (
    <ul data-testid="plugin-test-results" className="mt-2 space-y-2">
      {result.servers.map((server) => (
        <li key={server.name} className="rounded-md border border-border px-3 py-2">
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-mono text-xs text-text-primary">{server.name}</span>
            <span className="font-body text-xs text-text-muted">{server.kind}</span>
            <span className={statusClass(server.status)}>{server.status}</span>
          </div>
          {server.status === 'error' && (
            <p className="mt-1 font-body text-xs text-danger">
              {server.error ?? 'unknown error'}
            </p>
          )}
          {server.status === 'unsupported' && (
            <p className="mt-1 font-body text-xs text-warning">not supported</p>
          )}
          {server.status === 'ok' &&
            (server.tools.length === 0 ? (
              <p className="mt-1 font-body text-xs text-text-muted">No tools.</p>
            ) : (
              <ul className="mt-1 space-y-1">
                {server.tools.map((tool) => (
                  <li key={tool.exposedName} className="flex flex-wrap items-center gap-2">
                    <span className="font-mono text-xs text-text-primary">
                      {tool.exposedName}
                    </span>
                    {tool.readOnly && (
                      <span className="rounded-full border border-info-muted bg-info-muted px-2 py-0.5 font-body text-xs text-info">
                        read-only
                      </span>
                    )}
                    {tool.description && (
                      <span className="font-body text-xs text-text-secondary">
                        {tool.description}
                      </span>
                    )}
                  </li>
                ))}
              </ul>
            ))}
        </li>
      ))}
    </ul>
  );
}
