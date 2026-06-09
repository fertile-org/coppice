import type { ToolPart } from '../sync/types';

export function str(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

export function num(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

export function pathFromInput(input?: Record<string, unknown>): string {
  return str(input?.filePath) || str(input?.path) || str(input?.file);
}

export function outputText(part: ToolPart): string | undefined {
  if (typeof part.state.output === 'string' && part.state.output.trim()) {
    return part.state.output;
  }
  const meta = part.state.metadata;
  if (meta && typeof meta.output === 'string' && meta.output.trim()) {
    return meta.output;
  }
  return undefined;
}

export function formatOutput(output: string): string {
  return output.trim();
}
