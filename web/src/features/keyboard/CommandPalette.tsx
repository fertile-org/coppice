import { QueryClientContext } from '@tanstack/react-query';
import { useContext, useEffect, useMemo, useRef, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { useAgents } from '../agents/useAgents';
import { SessionContext } from '../auth/useSession';
import { useTickets } from '../board/useTickets';
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from '../../components/ui/command';
import { Dialog, DialogContent, DialogTitle } from '../../components/ui/dialog';
import { KeyboardContext } from './keyboardContext';
import { eventMatchesBinding } from './match';
import { useRegisterShortcut } from './useKeyboard';
import { detectPlatform } from './platform';
import { omittedActionIds, SHORTCUT_ACTIONS } from './registry';
import {
  boardIdFromPath,
  layoutPalette,
  listPaletteActions,
  PAGE_ACTION_ROUTES,
  PALETTE_COPY,
  paletteScopes,
  readRecentIds,
  rememberRecent,
  writeRecentIds,
  type PaletteAgent,
  type PaletteEntry,
  type PaletteTicket,
} from './palette';

function PaletteSources({
  boardId,
  onData,
}: {
  boardId: string | undefined;
  onData: (data: { tickets: PaletteTicket[]; agents: PaletteAgent[] }) => void;
}) {
  const ticketsQuery = useTickets(boardId);
  const agentsQuery = useAgents();
  const onDataRef = useRef(onData);
  useEffect(() => {
    onDataRef.current = onData;
  }, [onData]);

  useEffect(() => {
    onDataRef.current({
      tickets: (ticketsQuery.data ?? []).map((ticket) => ({
        id: ticket.id,
        title: ticket.title,
        boardId: ticket.boardId,
      })),
      agents: (agentsQuery.data ?? []).map((agent) => ({
        id: agent.id,
        name: agent.name,
      })),
    });
  }, [ticketsQuery.data, agentsQuery.data]);

  return null;
}

function PaletteRow({
  entry,
  onRun,
}: {
  entry: PaletteEntry;
  onRun: (entry: PaletteEntry) => void;
}) {
  return (
    <CommandItem value={entry.id} onSelect={() => onRun(entry)}>
      <span className="min-w-0 flex-1 truncate">{entry.label}</span>
      {entry.shortcut ? (
        <span className="ml-3 shrink-0 font-mono text-xs text-text-muted">{entry.shortcut}</span>
      ) : null}
    </CommandItem>
  );
}

export function CommandPalette() {
  const keyboard = useContext(KeyboardContext);
  const session = useContext(SessionContext);
  const location = useLocation();
  const navigate = useNavigate();
  const queryClient = useContext(QueryClientContext);
  const platform = useMemo(() => detectPlatform(), []);
  const actionsForKeys = keyboard?.actions ?? SHORTCUT_ACTIONS;
  const paletteAction = actionsForKeys.find((action) => action.id === 'palette.open');
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState('');
  const [recentIds, setRecentIds] = useState(() => readRecentIds(localStorage));
  const [catalog, setCatalog] = useState<{
    tickets: PaletteTicket[];
    agents: PaletteAgent[];
  }>({ tickets: [], agents: [] });
  const ranCommand = useRef(false);
  const restoreFocus = useRef<HTMLElement | null>(null);

  useRegisterShortcut('palette.open', () => {
    const active = document.activeElement;
    setOpen((current) => {
      if (current) return false;
      restoreFocus.current = active instanceof HTMLElement ? active : null;
      return true;
    });
  });

  useEffect(() => {
    if (!open) return;
    function onKeyDown(event: KeyboardEvent) {
      const matches = paletteAction?.bindings.some((binding) =>
        eventMatchesBinding(binding, event, platform),
      );
      if (!matches || event.repeat) return;
      event.preventDefault();
      event.stopPropagation();
      setOpen(false);
      setQuery('');
    }
    document.addEventListener('keydown', onKeyDown, true);
    return () => document.removeEventListener('keydown', onKeyDown, true);
  }, [open, paletteAction, platform]);

  const shortcutState = keyboard?.readShortcutState() ?? {
    enabledIds: [] as string[],
    drawerOpen: false,
  };
  const actions = listPaletteActions({
    omitted: omittedActionIds({
      isAdmin: session?.user?.role === 'admin',
      desktopMode: session?.desktopMode ?? false,
    }),
    scopes: paletteScopes({
      pathname: location.pathname,
      drawerOpen: shortcutState.drawerOpen,
    }),
    enabled: new Set(shortcutState.enabledIds),
    platform,
    actions: actionsForKeys,
  });
  const layout = layoutPalette({
    actions,
    tickets: catalog.tickets,
    agents: catalog.agents,
    recentIds,
    query,
  });
  const visible = [
    ...layout.recent,
    ...layout.groups.flatMap((group) => group.items),
    ...layout.matches,
  ];
  const highlighted = visible.some((entry) => entry.id === selected)
    ? selected
    : (visible[0]?.id ?? '');

  function runEntry(entry: PaletteEntry) {
    setRecentIds((current) => {
      const next = rememberRecent(current, entry.id);
      writeRecentIds(localStorage, next);
      return next;
    });
    const closesOnly = entry.id === 'close' || entry.id === 'palette.open';
    ranCommand.current = !closesOnly;
    if (entry.kind === 'ticket') {
      const ticket = catalog.tickets.find((item) => entry.id === `ticket:${item.id}`);
      if (ticket?.boardId) {
        navigate(`/boards/${ticket.boardId}?ticket=${ticket.id}`);
      }
    } else if (entry.kind === 'agent') {
      navigate('/agents');
    } else {
      const route = PAGE_ACTION_ROUTES[entry.id];
      if (route) navigate(route);
      else if (!closesOnly) keyboard?.runShortcut(entry.id);
    }
    setOpen(false);
    setQuery('');
  }

  return (
    <>
      {open && queryClient ? (
        <PaletteSources boardId={boardIdFromPath(location.pathname)} onData={setCatalog} />
      ) : null}
      <Dialog
        open={open}
        onOpenChange={(next) => {
          setOpen(next);
          if (!next) setQuery('');
        }}
      >
        <DialogContent
          className="z-[70] max-w-lg overflow-hidden p-0"
          overlayClassName="z-[70]"
          aria-describedby={undefined}
          onCloseAutoFocus={(event) => {
            const node = restoreFocus.current;
            restoreFocus.current = null;
            if (ranCommand.current) {
              ranCommand.current = false;
              event.preventDefault();
              return;
            }
            if (!(node instanceof HTMLElement) || !node.isConnected) return;
            event.preventDefault();
            node.focus();
          }}
        >
          <DialogTitle className="sr-only">{PALETTE_COPY.title}</DialogTitle>
          <Command
            shouldFilter={false}
            vimBindings={false}
            label={PALETTE_COPY.title}
            value={highlighted}
            onValueChange={setSelected}
          >
            <CommandInput
              autoFocus
              value={query}
              onValueChange={setQuery}
              placeholder={PALETTE_COPY.placeholder}
            />
            <CommandList>
              <CommandEmpty>{PALETTE_COPY.empty}</CommandEmpty>
              {layout.recent.length > 0 ? (
                <CommandGroup heading={PALETTE_COPY.recent}>
                  {layout.recent.map((entry) => (
                    <PaletteRow key={entry.id} entry={entry} onRun={runEntry} />
                  ))}
                </CommandGroup>
              ) : null}
              {layout.groups.map((group) => (
                <CommandGroup key={group.group} heading={group.group}>
                  {group.items.map((entry) => (
                    <PaletteRow key={entry.id} entry={entry} onRun={runEntry} />
                  ))}
                </CommandGroup>
              ))}
              {layout.matches.map((entry) => (
                <PaletteRow key={entry.id} entry={entry} onRun={runEntry} />
              ))}
            </CommandList>
          </Command>
        </DialogContent>
      </Dialog>
    </>
  );
}
