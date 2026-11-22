import { useEffect, useId, useRef, useState, type FormEvent, type KeyboardEvent } from 'react';
import { Button } from '../../components/ui/button';
import { Combobox } from '../../components/ui/combobox';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../../components/ui/dialog';
import { Input } from '../../components/ui/input';
import { Label } from '../../components/ui/label';
import { Textarea } from '../../components/ui/textarea';
import { parseApiErrorMessage } from '../../lib/api';
import {
  ticketPrioritySchema,
  type TicketPriorityValue,
} from '../../lib/schemas/ticket';
import type { CreateTicketInput } from './useTickets';

const PRIORITY_OPTIONS = ticketPrioritySchema.options.map((value) => ({
  value,
  label: value.charAt(0).toUpperCase() + value.slice(1),
}));

export type NewTicketDialogRepo = {
  id: string;
  name: string;
  verificationStatus?: string;
};

export function NewTicketDialog({
  open,
  repos,
  isPending,
  onClose,
  onCreate,
}: {
  open: boolean;
  repos: NewTicketDialogRepo[];
  isPending: boolean;
  onClose: () => void;
  onCreate: (input: CreateTicketInput) => Promise<unknown>;
}) {
  const titleId = useId();
  const descriptionId = useId();
  const repoIdField = useId();
  const priorityId = useId();
  const titleRef = useRef<HTMLInputElement>(null);

  const [title, setTitle] = useState('');
  const [description, setDescription] = useState('');
  const [repoId, setRepoId] = useState('');
  const [priority, setPriority] = useState<TicketPriorityValue>('medium');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) {
      setTitle('');
      setDescription('');
      setRepoId('');
      setPriority('medium');
      setError(null);
    }
  }, [open]);

  const trimmedTitle = title.trim();
  const canSubmit = trimmedTitle.length > 0 && !isPending;

  async function submit() {
    if (!canSubmit) return;
    setError(null);
    try {
      await onCreate({
        title: trimmedTitle,
        description,
        repoId: repoId || null,
        priority,
      });
      onClose();
    } catch (cause) {
      setError(parseApiErrorMessage(cause, 'Unable to create ticket.'));
    }
  }

  function handleSubmit(event: FormEvent) {
    event.preventDefault();
    void submit();
  }

  function handleKeyDown(event: KeyboardEvent) {
    if (event.key !== 'Enter') return;
    if (event.nativeEvent.isComposing) return;
    if (!(event.metaKey || event.ctrlKey)) return;
    event.preventDefault();
    void submit();
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next && !isPending) onClose();
      }}
    >
      <DialogContent
        className="max-w-lg"
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          titleRef.current?.focus();
        }}
        onKeyDown={handleKeyDown}
      >
        <form onSubmit={handleSubmit} noValidate>
          <DialogHeader>
            <DialogTitle>New ticket</DialogTitle>
          </DialogHeader>

          <div className="mt-5 space-y-4">
            <div className="space-y-1.5">
              <Label htmlFor={titleId}>Title</Label>
              <Input
                ref={titleRef}
                id={titleId}
                value={title}
                onChange={(event) => setTitle(event.target.value)}
                placeholder="What needs to happen?"
                required
                disabled={isPending}
              />
            </div>

            <div className="space-y-1.5">
              <Label htmlFor={descriptionId}>Description</Label>
              <Textarea
                id={descriptionId}
                value={description}
                onChange={(event) => setDescription(event.target.value)}
                placeholder="Context, constraints, acceptance criteria… (markdown)"
                rows={5}
                disabled={isPending}
              />
            </div>

            <div className="grid gap-4 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label htmlFor={repoIdField}>Repository</Label>
                <Combobox
                  id={repoIdField}
                  value={repoId}
                  onValueChange={setRepoId}
                  disabled={isPending}
                  placeholder="None"
                  searchPlaceholder="Search repositories…"
                  clearable
                  options={repos.map((repo) => ({
                    value: repo.id,
                    label:
                      repo.verificationStatus &&
                      repo.verificationStatus !== 'ready'
                        ? `${repo.name} (${repo.verificationStatus.replaceAll('_', ' ')})`
                        : repo.name,
                  }))}
                />
              </div>

              <div className="space-y-1.5">
                <Label htmlFor={priorityId}>Priority</Label>
                <Combobox
                  id={priorityId}
                  value={priority}
                  onValueChange={(value) =>
                    setPriority(value as TicketPriorityValue)
                  }
                  disabled={isPending}
                  options={PRIORITY_OPTIONS}
                />
              </div>
            </div>

            {error && (
              <p className="font-body text-sm text-danger" role="alert">
                {error}
              </p>
            )}
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="secondary"
              onClick={onClose}
              disabled={isPending}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={!canSubmit} loading={isPending}>
              {isPending ? 'Creating…' : 'Create ticket'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
