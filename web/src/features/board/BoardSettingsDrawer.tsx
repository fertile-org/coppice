import { Settings2 } from 'lucide-react';
import { Button } from '../../components/ui/button';
import { Input } from '../../components/ui/input';
import { Label } from '../../components/ui/label';
import { RepoDrawer } from '../repos/RepoDrawer';
import { useBoardRename } from './useBoardRename';
import type { ReviewCapacity, ReviewCapacityState } from './useReviewCapacity';

export interface BoardSettingsDrawerProps {
  open: boolean;
  boardId: string;
  capacity: ReviewCapacityState;
  onClose: () => void;
}

export function BoardSettingsDrawer({
  open,
  boardId,
  capacity,
  onClose,
}: BoardSettingsDrawerProps) {
  const { saved, draft, setDraft, error, commit, isPending } = capacity;
  const rename = useBoardRename(boardId, saved?.name);
  if (!saved || !draft) return null;

  // Escape and the backdrop can unmount the input before blur fires, so close
  // commits the draft too. A pending blur save already carries it.
  // Cap validation stays synchronous. Rename is async: wait for it (including a
  // blur already in flight) and only close when it succeeds.
  async function close() {
    if (!isPending && draft && !commit(draft)) return;
    try {
      await rename.commit(rename.draft);
    } catch {
      return;
    }
    onClose();
  }

  const nameId = `${boardId}-name`;
  const capId = `${boardId}-review-cap`;

  return (
    <RepoDrawer
      open={open}
      ariaLabel="Board settings"
      title={saved.boardSettingsLabel}
      onClose={() => {
        void close();
      }}
    >
      <div className="flex h-full flex-col gap-6">
        <div className="space-y-2">
          <Label htmlFor={nameId}>{saved.nameLabel}</Label>
          <Input
            id={nameId}
            value={rename.draft}
            onChange={(e) => rename.setDraft(e.target.value)}
            onBlur={() => {
              void rename.commit(rename.draft).catch(() => undefined);
            }}
            autoFocus
          />
        </div>

        <div className="space-y-2">
          <Label htmlFor={capId}>{saved.reviewCapLabel}</Label>
          <Input
            id={capId}
            type="number"
            min={1}
            step={1}
            value={draft.cap}
            disabled={isPending}
            onChange={(e) => setDraft({ ...draft, cap: e.target.value })}
            onBlur={() => commit(draft)}
            aria-describedby={`${capId}-hint`}
          />
          <p id={`${capId}-hint`} className="font-body text-xs text-text-muted">
            {saved.reviewCapHint}
          </p>
        </div>

        <label className="flex cursor-pointer items-center gap-2 font-body text-sm text-text-primary">
          <input
            type="checkbox"
            checked={draft.countInReview}
            disabled={isPending}
            onChange={(e) => {
              const next = { ...draft, countInReview: e.target.checked };
              setDraft(next);
              commit(next);
            }}
            className="h-4 w-4 rounded border-border text-moss-600 focus:ring-moss-500"
          />
          {saved.countInReviewLabel}
        </label>

        {(rename.error || error) && (
          <p className="font-body text-sm text-danger" role="alert">
            {rename.error ?? error}
          </p>
        )}

        <div className="mt-auto flex items-center justify-end gap-3 border-t border-border pt-4">
          <Button type="button" variant="secondary" onClick={close}>
            Done
          </Button>
        </div>
      </div>
    </RepoDrawer>
  );
}

export interface BoardSettingsButtonProps {
  saved: ReviewCapacity | undefined;
  onClick: () => void;
}

export function BoardSettingsButton({ saved, onClick }: BoardSettingsButtonProps) {
  if (!saved) return null;
  return (
    <Button type="button" variant="secondary" onClick={onClick}>
      <Settings2 className="size-4" aria-hidden="true" />
      {saved.boardSettingsLabel}
    </Button>
  );
}
