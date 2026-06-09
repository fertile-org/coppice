import type { Message } from '../sync/types';

function formatDuration(ms: number): string {
  const secs = Math.round(ms / 1000);
  if (secs < 60) return `${secs}s`;
  return `${Math.floor(secs / 60)}m ${secs % 60}s`;
}

export function sessionModelInfo(messages: Message[]): {
  mode?: string;
  modelID?: string;
  duration?: string;
} | null {
  const assistantMessages = messages.filter((m) => m.role === 'assistant');
  if (assistantMessages.length === 0) return null;

  const withModel = [...assistantMessages]
    .reverse()
    .find((m) => m.mode || m.modelID);
  if (!withModel?.mode && !withModel?.modelID) return null;

  const totalMs = assistantMessages.reduce((sum, message) => {
    const created = message.time?.created;
    const completed = message.time?.completed;
    if (created == null || completed == null) return sum;
    return sum + (completed - created);
  }, 0);

  return {
    mode: withModel.mode,
    modelID: withModel.modelID,
    duration: totalMs > 0 ? formatDuration(totalMs) : undefined,
  };
}
