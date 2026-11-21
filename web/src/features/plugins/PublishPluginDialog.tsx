import { useEffect, useState } from 'react';
import { Button } from '../../components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '../../components/ui/dialog';
import { BUILDER_COPY } from './builderCopy';

export function PublishPluginDialog({
  open,
  initialUrl,
  busy,
  error,
  branchExists,
  suggestedBranch,
  onClose,
  onPublish,
  onConfirmSuffix,
}: {
  open: boolean;
  initialUrl: string;
  busy: boolean;
  error: string | null;
  branchExists: boolean;
  suggestedBranch: string | null;
  onClose: () => void;
  onPublish: (url: string) => void;
  onConfirmSuffix: () => void;
}) {
  const [url, setUrl] = useState(initialUrl);
  useEffect(() => {
    if (open) setUrl(initialUrl);
  }, [open, initialUrl]);

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next && !busy) onClose();
      }}
    >
      <DialogContent
        className="z-[60] max-w-md"
        overlayClassName="z-[60]"
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          const panel = event.currentTarget;
          if (panel instanceof HTMLElement) {
            panel.querySelector<HTMLElement>('input')?.focus();
          }
        }}
      >
        <DialogTitle>
          {branchExists ? BUILDER_COPY.publishBranchExists(suggestedBranch ?? '') : BUILDER_COPY.publishDialogTitle}
        </DialogTitle>
        <DialogDescription className="mt-1">
          {branchExists
            ? null
            : BUILDER_COPY.publishDialogBody}
        </DialogDescription>
        {!branchExists && (
          <div className="mt-4 space-y-2">
            <label htmlFor="publish-plugin-url" className="font-body text-sm text-text-primary">
              {BUILDER_COPY.publishUrlLabel}
            </label>
            <input
              id="publish-plugin-url"
              type="url"
              value={url}
              disabled={busy}
              placeholder={BUILDER_COPY.publishUrlPlaceholder}
              onChange={(e) => setUrl(e.target.value)}
              className="w-full rounded-md border border-border bg-surface px-3 py-2 font-body text-sm text-text-primary"
            />
          </div>
        )}
        {error && (
          <p role="alert" className="mt-3 font-body text-sm text-danger">
            {error}
          </p>
        )}
        <div className="mt-5 flex justify-end gap-2">
          <Button
            type="button"
            variant="secondary"
            onClick={onClose}
            disabled={busy}
          >
            {BUILDER_COPY.publishCancel}
          </Button>
          {branchExists ? (
            <Button
              type="button"
              onClick={onConfirmSuffix}
              disabled={busy}
              loading={busy}
            >
              {BUILDER_COPY.publishPushSuffix}
            </Button>
          ) : (
            <Button
              type="button"
              onClick={() => onPublish(url.trim())}
              disabled={busy || !url.trim()}
              loading={busy}
            >
              {BUILDER_COPY.publishConfirm}
            </Button>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
