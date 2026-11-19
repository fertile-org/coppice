import { useState } from 'react';
import { Button } from '../../components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '../../components/ui/dialog';
import { BUILDER_COPY } from './builderCopy';

export function AcceptPluginDialog({
  open,
  publishTarget,
  busy,
  onClose,
  onConfirm,
}: {
  open: boolean;
  publishTarget: string | null;
  busy: boolean;
  onClose: () => void;
  onConfirm: (opts: { enable: boolean; publish: boolean }) => void;
}) {
  const [enable, setEnable] = useState(true);
  const [publish, setPublish] = useState(Boolean(publishTarget));

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
            panel.querySelector<HTMLElement>('[data-shortcut-cancel]')?.focus();
          }
        }}
      >
        <DialogTitle>{BUILDER_COPY.acceptDialogTitle}</DialogTitle>
        <DialogDescription className="mt-1">
          {BUILDER_COPY.acceptDialogBody}
        </DialogDescription>
        <div className="mt-4 space-y-3 font-body text-sm text-text-primary">
          <label className="flex items-start gap-2">
            <input
              type="checkbox"
              checked={enable}
              disabled={busy}
              onChange={(e) => setEnable(e.target.checked)}
              className="mt-0.5"
            />
            <span>{BUILDER_COPY.acceptEnable}</span>
          </label>
          {publishTarget && (
            <label className="flex items-start gap-2">
              <input
                type="checkbox"
                checked={publish}
                disabled={busy}
                onChange={(e) => setPublish(e.target.checked)}
                className="mt-0.5"
              />
              <span>{BUILDER_COPY.acceptPublish(publishTarget)}</span>
            </label>
          )}
        </div>
        <div className="mt-5 flex justify-end gap-2">
          <Button
            type="button"
            variant="secondary"
            data-shortcut-cancel=""
            onClick={onClose}
            disabled={busy}
          >
            {BUILDER_COPY.acceptCancel}
          </Button>
          <Button
            type="button"
            onClick={() => onConfirm({ enable, publish })}
            disabled={busy}
            loading={busy}
          >
            {BUILDER_COPY.accept}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
