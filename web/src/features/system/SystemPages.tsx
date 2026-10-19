import { useState, type ReactNode } from 'react';
import { Navigate, useLocation, useNavigate } from 'react-router-dom';
import { useToast } from '../../components/ToastProvider';
import { useSession } from '../auth/useSession';
import { setLastBoardId } from '../boards/useBoards';
import { InitWizard, type InitCreated } from '../init/InitWizard';
import { INIT_COPY } from '../init/copy';
import { setInitWizardFlag } from '../init/storage';
import { BackupTab } from '../tools/BackupTab';
import { ConnectorsTab } from '../tools/ConnectorsTab';
import { turnedOnToast } from '../tools/connectorCopy';
import { SYSTEM_PATHS } from './paths';

function SystemPageHeader({
  title,
  description,
  actions,
}: {
  title: string;
  description: string;
  actions?: ReactNode;
}) {
  return (
    <header className="flex flex-wrap items-start justify-between gap-4 border-b border-border pb-6">
      <div>
        <h1 className="font-display text-2xl font-semibold text-text-primary">{title}</h1>
        <p className="mt-2 max-w-2xl font-body text-sm text-text-secondary">{description}</p>
      </div>
      {actions}
    </header>
  );
}

export function BackupPage() {
  const { user } = useSession();
  if (user?.role !== 'admin') return <Navigate to="/boards" replace />;
  return (
    <div className="space-y-6">
      <SystemPageHeader
        title="Backup"
        description="Download a backup of this Coppice install, or restore one."
      />
      <BackupTab />
    </div>
  );
}

export function ConnectorsPage() {
  const { user, desktopMode } = useSession();
  const navigate = useNavigate();
  const toast = useToast();
  const [initOpen, setInitOpen] = useState(false);

  if (user?.role !== 'admin') return <Navigate to="/boards" replace />;

  function finishInit(created: InitCreated) {
    setInitWizardFlag('done');
    setLastBoardId(created.boardId);
    if (created.connectorTurnedOn) {
      toast.success(turnedOnToast(created.connectorDisplayName));
    }
    setInitOpen(false);
    navigate(`/boards/${created.boardId}`);
  }

  return (
    <div className="space-y-6">
      <SystemPageHeader
        title="Connectors"
        description="Check that agent CLIs are installed, signed in, and reach the Coppice gateway."
        actions={
          desktopMode ? (
            <button
              type="button"
              onClick={() => setInitOpen(true)}
              className="rounded-md border border-border px-3 py-1.5 font-body text-sm text-text-secondary transition-colors duration-fast hover:text-text-primary"
            >
              {INIT_COPY.toolsButton}
            </button>
          ) : null
        }
      />
      <ConnectorsTab />
      {desktopMode && (
        <InitWizard
          open={initOpen}
          onSkip={() => {
            setInitWizardFlag('skipped');
            setInitOpen(false);
          }}
          onCreated={finishInit}
        />
      )}
    </div>
  );
}

/** `/tools` and `/tools?tab=connectors` from older links and bookmarks. */
export function LegacyToolsRedirect() {
  const { search } = useLocation();
  const tab = new URLSearchParams(search).get('tab');
  return (
    <Navigate
      to={tab === 'connectors' ? SYSTEM_PATHS.connectors : SYSTEM_PATHS.backup}
      replace
    />
  );
}

/** `/logs` keeps its filters, which live in the query string. */
export function LegacyLogsRedirect() {
  const { search } = useLocation();
  return <Navigate to={`${SYSTEM_PATHS.logs}${search}`} replace />;
}
