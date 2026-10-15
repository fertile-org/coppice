import { useContext, useEffect, useRef } from 'react';
import {
  KeyboardContext,
  type KeyboardEnvironment,
  type LayerHandlers,
} from './keyboardContext';

export function useRegisterShortcut(
  id: string,
  run: () => void | boolean,
  enabled: boolean | (() => boolean) = true,
) {
  const ctx = useContext(KeyboardContext);
  const runRef = useRef(run);
  const enabledRef = useRef(enabled);

  useEffect(() => {
    runRef.current = run;
    enabledRef.current = enabled;
  }, [run, enabled]);

  useEffect(() => {
    if (!ctx) return;
    return ctx.register(id, {
      run: () => runRef.current(),
      enabled: () => {
        const current = enabledRef.current;
        return typeof current === 'function' ? current() : current;
      },
    });
  }, [ctx, id]);
}

export function useKeyboardEnvironment(environment: KeyboardEnvironment) {
  const ctx = useContext(KeyboardContext);
  const { drawerOpen, modalOpen } = environment;

  useEffect(() => {
    if (!ctx) return;
    ctx.setEnvironment({ drawerOpen, modalOpen });
    return () => ctx.setEnvironment({ drawerOpen: false, modalOpen: false });
  }, [ctx, drawerOpen, modalOpen]);
}

export function useKeyboardLayer(layer: LayerHandlers) {
  const ctx = useContext(KeyboardContext);
  const layerRef = useRef(layer);
  const hasClose = Boolean(layer.closeDrawer);
  const hasTabs = Boolean(layer.focusDrawerTabs);

  useEffect(() => {
    layerRef.current = layer;
  }, [layer]);

  useEffect(() => {
    if (!ctx) return;
    return ctx.registerLayer({
      closeDrawer: hasClose ? () => layerRef.current.closeDrawer?.() : undefined,
      focusDrawerTabs: hasTabs
        ? () => layerRef.current.focusDrawerTabs?.() ?? false
        : undefined,
    });
  }, [ctx, hasClose, hasTabs]);
}
