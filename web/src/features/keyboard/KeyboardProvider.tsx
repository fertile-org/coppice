import { useContext, useEffect, useMemo, useRef, type ReactNode } from 'react';
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
import { detectPlatform } from './platform';
import { omittedActionIds } from './registry';
import { CommandPalette } from './CommandPalette';
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
  const locationRef = useRef(location);
  const accessRef = useRef({
    isAdmin: session?.user?.role === 'admin',
    desktopMode: session?.desktopMode ?? false,
  });

  useEffect(() => {
    locationRef.current = location;
    accessRef.current = {
      isAdmin: session?.user?.role === 'admin',
      desktopMode: session?.desktopMode ?? false,
    };
  }, [location, session]);
  const handlersRef = useRef(new Map<string, ShortcutRegistration>());
  const layersRef = useRef<LayerHandlers>({});
  const envRef = useRef<KeyboardEnvironment>({
    drawerOpen: false,
    modalOpen: false,
  });
  const sequenceRef = useRef<SequenceSession>(EMPTY_SEQUENCE_SESSION);

  const api = useMemo<KeyboardContextValue>(
    () => ({
      register(id, registration) {
        handlersRef.current.set(id, registration);
        return () => {
          if (handlersRef.current.get(id) === registration) {
            handlersRef.current.delete(id);
          }
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
        for (const [id, registration] of handlersRef.current) {
          if (registration.enabled()) enabledIds.push(id);
        }
        return { enabledIds, drawerOpen: envRef.current.drawerOpen };
      },
      runShortcut(id) {
        const registration = handlersRef.current.get(id);
        if (!registration || !registration.enabled()) return false;
        return registration.run() !== false;
      },
    }),
    [],
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
      for (const [id, registration] of handlersRef.current) {
        if (registration.enabled()) enabled.add(id);
      }
      const reduced = reduceShortcut(
        sequenceRef.current,
        toKeyEvent(event),
        {
          platform,
          scopes,
          enabled,
          omitted: omittedActionIds(accessRef.current),
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

      const handled =
        reduced.effect.actionId === 'close'
          ? performClose(event.target)
          : handlersRef.current.get(reduced.effect.actionId)?.run() !== false;
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
      <CommandPalette />
    </KeyboardContext.Provider>
  );
}
