import type { ToolPart } from '../sync/types';
import { sessionTheme } from '../theme/session-theme';
import { str } from './tool-utils';
import { ToolShell } from './ToolShell';

interface ParsedQuestion {
  question: string;
}

function parseQuestions(value: unknown): ParsedQuestion[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item) => {
    if (typeof item !== 'object' || item === null) return [];
    const question = str((item as Record<string, unknown>).question);
    return question ? [{ question }] : [];
  });
}

function parseAnswers(value: unknown): string[][] | undefined {
  if (!Array.isArray(value)) return undefined;
  return value.map((answer) =>
    Array.isArray(answer) ? answer.filter((item): item is string => typeof item === 'string') : [],
  );
}

function formatAnswer(answer?: string[]): string {
  if (!answer?.length) return '(no answer)';
  return answer.join(', ');
}

export function Question({ part }: { part: ToolPart }) {
  const questions = parseQuestions(part.state.input?.questions);
  const answers = parseAnswers(part.state.metadata?.answers);
  const count = questions.length;
  const title =
    count > 0
      ? `Asked ${count} question${count !== 1 ? 's' : ''}`
      : 'Asking questions...';

  return (
    <ToolShell status={part.state.status} title={title}>
      <div className="flex flex-col gap-2">
        {questions.map((q, index) => (
          <div key={index} className="flex flex-col gap-0.5">
            <div className={`${sessionTheme.fontBody} ${sessionTheme.textMuted}`}>{q.question}</div>
            {answers ? (
              <div className={`${sessionTheme.fontBody} ${sessionTheme.text}`}>{formatAnswer(answers[index])}</div>
            ) : null}
          </div>
        ))}
      </div>
    </ToolShell>
  );
}
