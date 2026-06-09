import { sessionTheme } from '../theme/session-theme';
import { sessionModelInfo } from './session-footer';
import type { Message } from '../sync/types';

export function SessionFooter({ messages }: { messages: Message[] }) {
  const info = sessionModelInfo(messages);
  if (!info) return null;

  const { mode, modelID, duration } = info;

  return (
    <p className={`${sessionTheme.fontMonoXs} ${sessionTheme.textMuted}`}>
      {mode ? <span className={sessionTheme.mode}>{mode}</span> : null}
      {mode && modelID ? ' · ' : null}
      {modelID}
      {modelID && duration ? ' · ' : null}
      {duration}
    </p>
  );
}
