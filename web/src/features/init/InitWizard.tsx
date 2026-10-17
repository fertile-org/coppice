import { useEffect, useRef, useState, type FormEvent } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '../../components/ui/dialog';
import { Button } from '../../components/ui/button';
import { pickDirectory } from '../../lib/desktop';
import { useInspectRepoPath } from '../repos/useRepos';
import { SIGN_IN_UNCHECKED_HINT } from '../tools/connectorCopy';
import { useConnectors } from '../agents/useAgents';
import {
  INIT_COPY,
  joinNames,
  stepLabel,
  teamConnectorLine,
} from './copy';
import { initErrorCode, useConfirmInit, useInitPreview } from './useInitWizard';

type Step = 0 | 1 | 2;

export interface InitCreated {
  boardId: string;
  connectorTurnedOn: boolean;
  connectorDisplayName: string;
}

export function InitWizard({
  open,
  onSkip,
  onCreated,
}: {
  open: boolean;
  onSkip: () => void;
  onCreated: (created: InitCreated) => void;
}) {
  const created = useRef(false);
  const [step, setStep] = useState<Step>(0);
  const [localPath, setLocalPath] = useState('');
  const [name, setName] = useState('');
  const [remoteUrl, setRemoteUrl] = useState('');
  const [defaultBranch, setDefaultBranch] = useState('main');
  const [verification, setVerification] = useState<string | null>(null);
  const [boardName, setBoardName] = useState('');
  const [connectorId, setConnectorId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const inspectPath = useInspectRepoPath();
  const confirm = useConfirmInit();
  const preview = useInitPreview(open);
  const connectors = useConnectors({ enabled: open });

  useEffect(() => {
    if (!open) return;
    created.current = false;
    setStep(0);
    setLocalPath('');
    setName('');
    setRemoteUrl('');
    setDefaultBranch('main');
    setVerification(null);
    setBoardName('');
    setConnectorId(null);
    setError(null);
  }, [open]);

  function handleOpenChange(next: boolean) {
    if (next) return;
    if (created.current) {
      created.current = false;
      return;
    }
    onSkip();
  }

  async function applyInspect(path: string) {
    const trimmed = path.trim();
    if (!trimmed) return;
    setError(null);
    try {
      const inspected = await inspectPath.mutateAsync(trimmed);
      setVerification(inspected.verificationStatus);
      setName((current) => current.trim() || inspected.suggestedName || current);
      setRemoteUrl((current) => current.trim() || inspected.remoteUrl || current);
      if (inspected.defaultBranch) setDefaultBranch(inspected.defaultBranch);
    } catch {
      setVerification(null);
      setError(INIT_COPY.inspectFailed);
    }
  }

  async function handleBrowse() {
    const selected = await pickDirectory();
    if (!selected) return;
    setLocalPath(selected);
    await applyInspect(selected);
  }

  function continueFromRepository() {
    if (!localPath.trim()) {
      setError(INIT_COPY.pathRequired);
      return;
    }
    if (!name.trim() || !defaultBranch.trim() || verification !== 'ready') {
      setError(INIT_COPY.notGit);
      return;
    }
    setError(null);
    setBoardName((current) => current.trim() || name.trim());
    setStep(1);
  }

  function continueFromBoard() {
    if (!boardName.trim()) {
      setError(INIT_COPY.boardNameRequired);
      return;
    }
    setError(null);
    setStep(2);
  }

  async function handleCreate() {
    if (!chosen) return;
    setError(null);
    try {
      const result = await confirm.mutateAsync({
        boardName: boardName.trim(),
        connectorId: chosen.id,
        repository: {
          name: name.trim(),
          localPath: localPath.trim(),
          remoteUrl: remoteUrl.trim() ? remoteUrl.trim() : null,
          defaultBranch: defaultBranch.trim(),
        },
      });
      created.current = true;
      onCreated({
        boardId: result.boardId,
        connectorTurnedOn: result.connectorTurnedOn,
        connectorDisplayName: result.connectorDisplayName,
      });
    } catch (err) {
      const code = initErrorCode(err);
      if (code === 'duplicate_board') setError(INIT_COPY.duplicateBoard);
      else if (code === 'not_git') setError(INIT_COPY.notGit);
      else if (code === 'checking') setError(INIT_COPY.checking);
      else if (code === 'none_ready') setError(INIT_COPY.noReady);
      else setError(INIT_COPY.saveFailed);
    }
  }

  function handleSubmit(event: FormEvent) {
    event.preventDefault();
    if (step === 0) continueFromRepository();
    else if (step === 1) continueFromBoard();
    else void handleCreate();
  }

  const stepName = step === 0 ? INIT_COPY.repository : step === 1 ? INIT_COPY.board : INIT_COPY.team;
  const pathStatus = verificationLabel(verification);
  const members = preview.data?.members.map((member) => member.name) ?? [];
  const readyConnectors = preview.data?.status === 'ready' ? preview.data.connectors : [];
  const chosen =
    readyConnectors.find((connector) => connector.id === connectorId) ?? readyConnectors[0] ?? null;
  const kiloUnchecked =
    preview.data?.status === 'none' &&
    connectors.data?.some(
      (connector) => connector.id === 'kilo-code' && connector.readiness === 'found',
    );
  const repositoryReady =
    Boolean(localPath.trim()) &&
    Boolean(name.trim()) &&
    Boolean(defaultBranch.trim()) &&
    verification === 'ready' &&
    !inspectPath.isPending;
  const createReady = preview.data?.status === 'ready' && Boolean(chosen);

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogTitle>{INIT_COPY.title}</DialogTitle>
        <DialogDescription className="mt-1">{INIT_COPY.intro}</DialogDescription>

        <form onSubmit={handleSubmit} className="mt-5 space-y-4">
          <div>
            <p className="font-body text-xs text-text-muted">{stepLabel(step + 1)}</p>
            <h2 className="font-display text-lg font-semibold text-bark-900">{stepName}</h2>
          </div>

          {step === 0 && (
            <div className="space-y-3">
              <div>
                <div className="mb-1 flex items-center justify-between gap-2">
                  <label
                    htmlFor="init-local-path"
                    className="block font-body text-sm font-medium text-bark-800"
                  >
                    {INIT_COPY.localPath}
                  </label>
                  <Button
                    type="button"
                    variant="secondary"
                    onClick={() => void handleBrowse()}
                    disabled={inspectPath.isPending}
                  >
                    {INIT_COPY.browse}
                  </Button>
                </div>
                <input
                  id="init-local-path"
                  type="text"
                  value={localPath}
                  onChange={(event) => {
                    setLocalPath(event.target.value);
                    setVerification(null);
                  }}
                  onBlur={() => void applyInspect(localPath)}
                  className="field-control w-full px-3 py-2 font-mono text-sm"
                />
                {pathStatus && (
                  <p className="mt-1 font-body text-xs text-text-secondary">{pathStatus}</p>
                )}
              </div>
              <Field
                id="init-repo-name"
                label={INIT_COPY.name}
                value={name}
                onChange={setName}
              />
              <div>
                <label
                  htmlFor="init-remote-url"
                  className="mb-1 block font-body text-sm font-medium text-bark-800"
                >
                  {INIT_COPY.remoteUrl}{' '}
                  <span className="font-normal text-text-muted">{INIT_COPY.remoteOptional}</span>
                </label>
                <input
                  id="init-remote-url"
                  type="text"
                  autoComplete="off"
                  spellCheck={false}
                  value={remoteUrl}
                  onChange={(event) => setRemoteUrl(event.target.value)}
                  className="field-control w-full px-3 py-2 font-mono text-sm"
                />
              </div>
              <Field
                id="init-default-branch"
                label={INIT_COPY.defaultBranch}
                value={defaultBranch}
                onChange={setDefaultBranch}
              />
            </div>
          )}

          {step === 1 && (
            <Field
              id="init-board-name"
              label={INIT_COPY.name}
              value={boardName}
              onChange={setBoardName}
            />
          )}

          {step === 2 && (
            <div className="space-y-2">
              <h3 className="font-body text-sm font-medium text-bark-800">
                {INIT_COPY.teamHeading}
              </h3>
              {preview.isLoading && (
                <p className="font-body text-sm text-text-secondary">{INIT_COPY.loading}</p>
              )}
              {preview.isError && (
                <p className="font-body text-sm text-danger">{INIT_COPY.loadFailed}</p>
              )}
              {preview.data && members.length > 0 && (
                <p className="font-body text-sm text-text-secondary">{joinNames(members)}.</p>
              )}
              {chosen && (
                <>
                  <label
                    htmlFor="init-agent-cli"
                    className="block font-body text-sm font-medium text-bark-800"
                  >
                    {INIT_COPY.agentCli}
                  </label>
                  <select
                    id="init-agent-cli"
                    value={chosen.id}
                    onChange={(event) => setConnectorId(event.target.value)}
                    className="field-control w-full px-3 py-2 font-body text-sm"
                  >
                    {readyConnectors.map((connector) => (
                      <option key={connector.id} value={connector.id}>
                        {connector.displayName}
                      </option>
                    ))}
                  </select>
                  <p className="font-body text-sm text-text-secondary">
                    {teamConnectorLine(chosen.displayName)}
                  </p>
                  <p className="font-body text-sm text-text-secondary">{INIT_COPY.teamIdle}</p>
                </>
              )}
              {preview.data?.status === 'checking' && (
                <p className="font-body text-sm text-text-secondary">{INIT_COPY.checking}</p>
              )}
              {preview.data?.status === 'none' && (
                <p className="font-body text-sm text-text-secondary">{INIT_COPY.noReady}</p>
              )}
              {kiloUnchecked && (
                <p className="font-body text-sm text-text-secondary">{SIGN_IN_UNCHECKED_HINT}</p>
              )}
            </div>
          )}

          {error && (
            <p
              role="alert"
              className="rounded-md bg-danger-muted px-3 py-2 font-body text-sm text-danger"
            >
              {error}
            </p>
          )}

          <div className="flex justify-end gap-2 pt-1">
            <button
              type="button"
              onClick={onSkip}
              className="mr-auto rounded-md border border-border px-4 py-2 font-body text-sm text-text-secondary transition-colors duration-fast hover:border-bark-300 hover:text-text-primary"
            >
              {INIT_COPY.skip}
            </button>
            {step > 0 && (
              <button
                type="button"
                onClick={() => {
                  setError(null);
                  setStep((current) => (current === 2 ? 1 : 0));
                }}
                className="rounded-md border border-border px-4 py-2 font-body text-sm text-text-secondary transition-colors duration-fast hover:border-bark-300 hover:text-text-primary"
              >
                {INIT_COPY.back}
              </button>
            )}
            {step < 2 ? (
              <button
                type="submit"
                disabled={step === 0 ? !repositoryReady : !boardName.trim()}
                className="rounded-md bg-moss-600 px-4 py-2 font-body text-sm font-medium text-paper-50 transition-colors duration-fast hover:bg-moss-700 disabled:opacity-60"
              >
                {INIT_COPY.next}
              </button>
            ) : (
              <button
                type="submit"
                disabled={!createReady || confirm.isPending}
                className="rounded-md bg-moss-600 px-4 py-2 font-body text-sm font-medium text-paper-50 transition-colors duration-fast hover:bg-moss-700 disabled:opacity-60"
              >
                {confirm.isPending ? INIT_COPY.creating : INIT_COPY.create}
              </button>
            )}
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function Field({
  id,
  label,
  value,
  onChange,
}: {
  id: string;
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <div>
      <label htmlFor={id} className="mb-1 block font-body text-sm font-medium text-bark-800">
        {label}
      </label>
      <input
        id={id}
        type="text"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="field-control w-full px-3 py-2 font-body text-sm"
      />
    </div>
  );
}

function verificationLabel(status: string | null): string | null {
  switch (status) {
    case 'ready':
      return INIT_COPY.pathReady;
    case 'path_missing':
      return INIT_COPY.pathMissing;
    case 'not_git_repo':
      return INIT_COPY.notGitRepo;
    case 'error':
      return INIT_COPY.pathError;
    default:
      return null;
  }
}
