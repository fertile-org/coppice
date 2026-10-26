import { ArrowLeft } from 'lucide-react';
import { useLocation, useNavigate } from 'react-router-dom';
import { Button } from '../../components/ui/button';
import { codeReviewOrigin } from './openCodeReview';

export function CodeReviewBackButton({ fallbackHref }: { fallbackHref: string }) {
  const location = useLocation();
  const navigate = useNavigate();
  const target = codeReviewOrigin(location.state) ?? fallbackHref;

  return (
    <Button
      type="button"
      variant="ghost"
      className="shrink-0"
      onClick={() => navigate(target)}
    >
      <ArrowLeft className="size-4" aria-hidden="true" />
      Back
    </Button>
  );
}