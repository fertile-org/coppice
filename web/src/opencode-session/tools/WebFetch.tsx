import { PlainOutput } from '../components/PlainOutput';
import type { ToolPart } from '../sync/types';
import { sessionTheme } from '../theme/session-theme';
import { excerptWebContent, isMostlyHtml } from './parse-opencode-output';
import { outputText, str } from './tool-utils';
import { ToolOutput, ToolShell } from './ToolShell';

export function WebFetch({ part }: { part: ToolPart }) {
  const url = str(part.state.input?.url);
  const raw = outputText(part);
  const excerpt = raw ? excerptWebContent(raw) : undefined;
  const html = raw ? isMostlyHtml(raw) : false;

  return (
    <ToolShell status={part.state.status} title={url ? `Fetch ${url}` : 'Fetch'}>
      {excerpt ? (
        <ToolOutput>
          {html && (
            <p className={`mb-2 ${sessionTheme.fontBody} ${sessionTheme.textMuted}`}>
              Fetched page — showing text excerpt ({raw?.length.toLocaleString()} chars)
            </p>
          )}
          <PlainOutput text={excerpt} />
        </ToolOutput>
      ) : null}
    </ToolShell>
  );
}
