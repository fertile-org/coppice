import { MarkdownContent } from '../components/MarkdownContent';
import type { TextPart as TextPartType } from '../sync/types';
import { useTypewriter } from '../../features/runs/useTypewriter';
import { AgentResultCard } from './AgentResultCard';
import { parseResultContractFromText } from './parse-result-contract';

export function TextPart({
  part,
  streaming = false,
}: {
  part: TextPartType;
  streaming?: boolean;
}) {
  const content = part.text.trim();
  const revealed = useTypewriter(content, { enabled: streaming });
  if (!content) return null;

  const contract = parseResultContractFromText(content);
  if (contract) {
    return <AgentResultCard contract={contract} />;
  }

  return (
    <div>
      <MarkdownContent>{revealed}</MarkdownContent>
    </div>
  );
}
