import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useToast } from '../../components/ToastProvider';
import { turnedOnToast } from '../tools/connectorCopy';
import { useSession } from '../auth/useSession';
import { setLastBoardId, useBoards } from '../boards/useBoards';
import { useAgents } from '../agents/useAgents';
import { useRepos } from '../repos/useRepos';
import { BrandLogo } from '../../components/BrandLogo';
import { INIT_COPY } from './copy';
import { InitWizard, type InitCreated } from './InitWizard';
import { initWizardFlag, setInitWizardFlag } from './storage';

/** Offers the wizard once on an empty desktop workspace, as a welcome page. */
export function InitWizardOffer() {
  const { desktopMode, user } = useSession();
  const navigate = useNavigate();
  const toast = useToast();
  const [open, setOpen] = useState(false);
  const [offered, setOffered] = useState(false);
  const [settled, setSettled] = useState(false);
  // Candidate before boards load so repos/agents start in parallel with boards.
  const candidate =
    desktopMode && user?.role === 'admin' && initWizardFlag() == null;
  const boards = useBoards();
  const repos = useRepos({ enabled: candidate });
  const agents = useAgents({ enabled: candidate });
  const pending = candidate && !settled;

  useEffect(() => {
    if (!candidate) {
      setSettled(true);
      return;
    }
    if (offered) return;

    if (boards.isError || repos.isError || agents.isError) {
      setSettled(true);
      return;
    }

    const boardsDone = boards.isFetched;
    const reposDone = repos.isFetched;
    const agentsDone = agents.isFetched;
    const hasBoards = boardsDone && (boards.data?.length ?? 0) > 0;
    const hasRepos = reposDone && (repos.data?.length ?? 0) > 0;
    const hasAgents = agentsDone && (agents.data?.length ?? 0) > 0;
    if (hasBoards || hasRepos || hasAgents) {
      setSettled(true);
      return;
    }

    if (!boardsDone || !reposDone || !agentsDone) return;

    setSettled(true);
    setOffered(true);
    setOpen(true);
  }, [
    agents.data,
    agents.isError,
    agents.isFetched,
    boards.data,
    boards.isError,
    boards.isFetched,
    candidate,
    offered,
    repos.data,
    repos.isError,
    repos.isFetched,
  ]);

  function finish(created: InitCreated) {
    setInitWizardFlag('done');
    setLastBoardId(created.boardId);
    if (created.connectorTurnedOn) {
      toast.success(turnedOnToast(created.connectorDisplayName));
    }
    setOpen(false);
    navigate(`/boards/${created.boardId}`);
  }

  if (!desktopMode || user?.role !== 'admin') return null;

  return (
    <>
      <div
        data-testid="init-offer"
        data-settled={settled ? 'yes' : 'no'}
        data-open={open ? 'yes' : 'no'}
        hidden
      />
      {pending ? (
        // Covers the app shell while empty-workspace checks finish.
        <main
          className="coppice-grain fixed inset-0 z-50 overflow-y-auto bg-background"
          data-testid="init-cover"
          aria-busy="true"
        >
          <div className="mx-auto flex min-h-full w-full max-w-lg flex-col items-center justify-center px-4 py-12">
            <BrandLogo size={72} className="h-[72px] w-[72px]" />
          </div>
        </main>
      ) : open ? (
        // Covers the app shell: a new user sees a welcome, not an empty board.
        <main
          className="coppice-grain fixed inset-0 z-50 overflow-y-auto bg-background"
          data-testid="init-welcome"
        >
          <div className="mx-auto flex min-h-full w-full max-w-lg flex-col items-center px-4 py-12 text-center">
            <BrandLogo size={72} className="h-[72px] w-[72px]" />
            <h1 className="mt-4 font-display text-3xl font-semibold tracking-tight text-text-primary">
              {INIT_COPY.welcomeTitle}
            </h1>
            <p className="mt-2 max-w-md font-body text-sm text-text-secondary">
              {INIT_COPY.welcomeIntro}
            </p>
            <div className="mt-8 flex w-full justify-center">
              <InitWizard
                open
                variant="page"
                onSkip={() => {
                  setInitWizardFlag('skipped');
                  setOpen(false);
                }}
                onCreated={finish}
              />
            </div>
          </div>
        </main>
      ) : null}
    </>
  );
}
