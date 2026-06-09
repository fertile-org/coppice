import { MarkdownContent } from '../components/MarkdownContent';
import type { TextPart as TextPartType } from '../sync/types';
import { AgentResultCard } from './AgentResultCard';
import { parseResultContractFromText } from './parse-result-contract';

export function TextPart({ part }: { part: TextPartType }) {
  const content = part.text.trim();
  if (!content) return null;

  const contract = parseResultContractFromText(content);
  if (contract) {
    return <AgentResultCard contract={contract} />;
  }

  return (
    <div>
      <MarkdownContent>{content}</MarkdownContent>
    </div>
  );
}
