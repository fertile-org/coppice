import { useId, useState } from 'react';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';
import { useToast } from '../../components/ToastProvider';
import { useSession } from '../auth/useSession';
import { setLastBoardId } from '../boards/useBoards';
import { InitWizard, type InitCreated } from '../init/InitWizard';
import { INIT_COPY } from '../init/copy';
import { setInitWizardFlag } from '../init/storage';
import { turnedOnToast } from './connectorCopy';
import { BackupTab } from './BackupTab';
import { ConnectorsTab } from './ConnectorsTab';

type ToolsTab = 'backup' | 'connectors';

const TOOLS_TABS: { key: ToolsTab; label: string }[] = [
  { key: 'backup', label: 'Backup' },
  { key: 'connectors', label: 'Connectors' },
];

export function ToolsPage() {
  const { user, desktopMode } = useSession();
  const navigate = useNavigate();
  const toast = useToast();
  const [initOpen, setInitOpen] = useState(false);
  const [searchParams, setSearchParams] = useSearchParams();
  const idPrefix = useId();
  const tab: ToolsTab = searchParams.get('tab') === 'connectors' ? 'connectors' : 'backup';
  const tabId = (key: ToolsTab) => `${idPrefix}-tab-${key}`;
  const panelId = (key: ToolsTab) => `${idPrefix}-panel-${key}`;

  if (user?.role !== 'admin') {
    return <Navigate to="/boards" replace />;
  }

  function selectTab(key: ToolsTab) {
    setSearchParams(key === 'backup' ? {} : { tab: key }, { replace: true });
  }

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
      <header className="flex flex-wrap items-start justify-between gap-4 border-b border-border pb-6">
        <div>
          <h1 className="font-display text-2xl font-semibold text-text-primary">
            Tools
          </h1>
          <p className="mt-2 max-w-2xl font-body text-sm text-text-secondary">
            Back up this Coppice instance and check that agent connectors are
            installed, logged in, and reach the Coppice gateway.
          </p>
        </div>
        {desktopMode && (
          <button
            type="button"
            onClick={() => setInitOpen(true)}
            className="rounded-md border border-border px-3 py-1.5 font-body text-sm text-text-secondary transition-colors duration-fast hover:text-text-primary"
          >
            {INIT_COPY.toolsButton}
          </button>
        )}
      </header>

      <div role="tablist" aria-label="Tools" className="flex gap-1 border-b border-border">
        {TOOLS_TABS.map(({ key, label }) => (
          <button
            key={key}
            id={tabId(key)}
            type="button"
            role="tab"
            aria-selected={tab === key}
            aria-controls={panelId(key)}
            onClick={() => selectTab(key)}
            className={[
              'border-b-2 px-3 py-2 font-body text-sm transition-colors duration-fast',
              tab === key
                ? 'border-accent text-accent'
                : 'border-transparent text-text-secondary hover:text-text-primary',
            ].join(' ')}
          >
            {label}
          </button>
        ))}
      </div>

      {TOOLS_TABS.map(({ key }) => (
        <div
          key={key}
          id={panelId(key)}
          role="tabpanel"
          aria-labelledby={tabId(key)}
          hidden={tab !== key}
        >
          {tab === key && (key === 'backup' ? <BackupTab /> : <ConnectorsTab />)}
        </div>
      ))}

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
