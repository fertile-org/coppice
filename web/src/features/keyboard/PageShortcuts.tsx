import { useContext, useEffect, useRef } from 'react';
import { useNavigate } from 'react-router-dom';
import { KeyboardContext } from './keyboardContext';
import { PAGE_ACTION_ROUTES } from './palette';

export function PageShortcuts() {
  const keyboard = useContext(KeyboardContext);
  const navigate = useNavigate();
  const navigateRef = useRef(navigate);

  useEffect(() => {
    navigateRef.current = navigate;
  }, [navigate]);

  useEffect(() => {
    if (!keyboard) return;
    const cleanups = Object.entries(PAGE_ACTION_ROUTES).map(([id, route]) =>
      keyboard.register(id, {
        run: () => {
          navigateRef.current(route);
        },
        enabled: () => true,
      }),
    );
    return () => {
      for (const cleanup of cleanups) cleanup();
    };
  }, [keyboard]);

  return null;
}
