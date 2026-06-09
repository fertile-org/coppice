import { PlainOutput } from '../components/PlainOutput';
import type { ToolPart } from '../sync/types';
import { excerptWebContent, isMostlyHtml } from './parse-opencode-output';
import { outputText, str } from './tool-utils';
import { ToolOutput, ToolShell } from './ToolShell';

export function WebFetch({ part }: { part: ToolPart }) {
  const url = str(part.state.input?.url);
  const raw = outputText(part);
  const excerpt = raw ? excerptWebContent(raw) : undefined;
  const html = raw ? isMostlyHtml(raw) : false;

  return (
    <ToolShell tool="webfetch" status={part.state.status} title={url || 'webfetch'}>
      {excerpt ? (
        <ToolOutput>
          {html && (
            <p className="mb-2 font-body text-xs text-text-muted">
              Fetched page — showing text excerpt ({raw?.length.toLocaleString()} chars)
            </p>
          )}
          <PlainOutput text={excerpt} />
        </ToolOutput>
      ) : null}
    </ToolShell>
  );
}
