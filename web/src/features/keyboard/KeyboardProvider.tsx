import { useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { useLocation } from 'react-router-dom';
import { SessionContext } from '../auth/useSession';
import {
  KeyboardContext,
  type KeyboardContextValue,
  type KeyboardEnvironment,
  type LayerHandlers,
  type ShortcutRegistration,
} from './keyboardContext';
import {
  activeScopes,
  EMPTY_SEQUENCE_SESSION,
  reduceShortcut,
  type SequenceSession,
} from './match';
import {
  currentKeybindingWarnings,
  currentKeybindings,
  effectiveShortcutActions,
  publishKeybindings,
  readKeybindings,
  subscribeKeybindings,
} from './keybindings';
import { detectPlatform } from './platform';
import { omittedActionIds } from './registry';
import { CommandPalette } from './CommandPalette';
import { PageShortcuts } from './PageShortcuts';
import { ShortcutHelp } from './ShortcutHelp';
import { classifyTarget, isForeignDialog } from './targets';
import type { KeyEventLike } from './types';

function toKeyEvent(event: KeyboardEvent): KeyEventLike {
  return {
    key: event.key,
    code: event.code,
    metaKey: event.metaKey,
    ctrlKey: event.ctrlKey,
    altKey: event.altKey,
    shiftKey: event.shiftKey,
    repeat: event.repeat,
    isComposing: event.isComposing,
  };
}

export function KeyboardProvider({ children }: { children: ReactNode }) {
  const location = useLocation();
  const session = useContext(SessionContext);
  const platform = useMemo(() => detectPlatform(), []);
  const [overrides, setOverrides] = useState(currentKeybindings);
  const [warnings, setWarnings] = useState(currentKeybindingWarnings);
  const actions = useMemo(() => effectiveShortcutActions(overrides), [overrides]);
  const actionsRef = useRef(actions);
  const locationRef = useRef(location);
  const accessRef = useRef({
    isAdmin: session?.user?.role === 'admin',
    desktopMode: session?.desktopMode ?? false,
  });

  useEffect(() => {
    actionsRef.current = actions;
    locationRef.current = location;
    accessRef.current = {
      isAdmin: session?.user?.role === 'admin',
      desktopMode: session?.desktopMode ?? false,
    };
  }, [actions, location, session]);

  useEffect(
    () =>
      subscribeKeybindings((next) => {
        setOverrides(next.overrides);
        setWarnings(next.warnings);
      }),
    [],
  );

  useEffect(() => {
    // Tests publish overrides directly. A relative fetch here has no server.
    if (!session?.user || import.meta.env.VITEST) return;
    let cancelled = false;
    readKeybindings()
      .then((next) => {
        if (!cancelled) publishKeybindings(next.overrides, next.warnings);
      })
      .catch(() => {
        /* Keep the shortcuts already in effect. */
      });
    return () => {
      cancelled = true;
    };
  }, [session?.user]);
  const handlersRef = useRef(new Map<string, ShortcutRegistration[]>());
  const layersRef = useRef<LayerHandlers>({});
  const envRef = useRef<KeyboardEnvironment>({
    drawerOpen: false,
    modalOpen: false,
  });
  const sequenceRef = useRef<SequenceSession>(EMPTY_SEQUENCE_SESSION);

  const api = useMemo<KeyboardContextValue>(
    () => ({
      actions,
      keybindingWarnings: warnings,
      register(id, registration) {
        const list = handlersRef.current.get(id) ?? [];
        handlersRef.current.set(id, [...list, registration]);
        return () => {
          const current = handlersRef.current.get(id);
          if (!current) return;
          const next = current.filter((item) => item !== registration);
          if (next.length === 0) handlersRef.current.delete(id);
          else handlersRef.current.set(id, next);
        };
      },
      registerLayer(layer) {
        const previous = layersRef.current;
        const next = { ...previous };
        if (layer.closeDrawer) next.closeDrawer = layer.closeDrawer;
        if (layer.focusDrawerTabs) next.focusDrawerTabs = layer.focusDrawerTabs;
        layersRef.current = next;
        return () => {
          const current = layersRef.current;
          const restored = { ...current };
          if (layer.closeDrawer && current.closeDrawer === layer.closeDrawer) {
            delete restored.closeDrawer;
          }
          if (
            layer.focusDrawerTabs &&
            current.focusDrawerTabs === layer.focusDrawerTabs
          ) {
            delete restored.focusDrawerTabs;
          }
          layersRef.current = restored;
        };
      },
      setEnvironment(environment) {
        envRef.current = environment;
      },
      readShortcutState() {
        const enabledIds = ['close'];
        for (const [id, list] of handlersRef.current) {
          if (list.some((registration) => registration.enabled())) enabledIds.push(id);
        }
        return { enabledIds, drawerOpen: envRef.current.drawerOpen };
      },
      runShortcut(id) {
        const registration = handlersRef.current
          .get(id)
          ?.find((item) => item.enabled());
        if (!registration) return false;
        return registration.run() !== false;
      },
    }),
    [actions, warnings],
  );

  useEffect(() => {
    function performClose(target: EventTarget | null): boolean {
      if (isForeignDialog(target)) return false;
      const kind = classifyTarget(target);
      if (kind === 'terminal') {
        return layersRef.current.focusDrawerTabs?.() ?? false;
      }
      if (kind === 'text' && target instanceof HTMLElement) {
        target.blur();
        return true;
      }
      if (envRef.current.drawerOpen && layersRef.current.closeDrawer) {
        layersRef.current.closeDrawer();
        return true;
      }
      return false;
    }

    function onKeyDown(event: KeyboardEvent) {
      const environment = envRef.current;
      const foreignDialog = isForeignDialog(event.target);
      const scopes = activeScopes({
        pathname: locationRef.current.pathname,
        drawerOpen: environment.drawerOpen,
        modalOpen: environment.modalOpen,
        targetKind: classifyTarget(event.target),
        foreignDialog,
      });
      const enabled = new Set<string>(['close']);
      for (const [id, list] of handlersRef.current) {
        if (list.some((registration) => registration.enabled())) enabled.add(id);
      }
      const reduced = reduceShortcut(
        sequenceRef.current,
        toKeyEvent(event),
        {
          platform,
          scopes,
          enabled,
          omitted: omittedActionIds(accessRef.current),
          actions: actionsRef.current,
        },
        Date.now(),
      );
      sequenceRef.current = reduced.session;
      if (reduced.effect.type === 'none') return;

      if (reduced.effect.type === 'swallow') {
        event.preventDefault();
        event.stopPropagation();
        return;
      }

      const registration = handlersRef.current
        .get(reduced.effect.actionId)
        ?.find((item) => item.enabled());
      const handled =
        reduced.effect.actionId === 'close'
          ? performClose(event.target)
          : registration != null && registration.run() !== false;
      if (!handled) return;
      event.preventDefault();
      event.stopPropagation();
    }

    document.addEventListener('keydown', onKeyDown, true);
    return () => document.removeEventListener('keydown', onKeyDown, true);
  }, [platform]);

  return (
    <KeyboardContext.Provider value={api}>
      {children}
      <PageShortcuts />
      <ShortcutHelp />
      <CommandPalette />
    </KeyboardContext.Provider>
  );
}
