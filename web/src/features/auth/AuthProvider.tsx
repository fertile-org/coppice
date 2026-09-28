import { useCallback, useEffect, useState, type ReactNode } from 'react';
import { apiFetch, setCsrfToken } from '../../lib/api';
import { SessionContext, type User } from './useSession';

interface MeResponse {
  user: User;
  csrfToken: string;
}

interface CapabilitiesResponse {
  desktopMode: boolean;
}

interface AuthProviderProps {
  children: ReactNode;
}

export function AuthProvider({ children }: AuthProviderProps) {
  const [user, setUser] = useState<User | null>(null);
  const [desktopMode, setDesktopMode] = useState(false);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;

    async function loadSession() {
      let nextDesktopMode = false;
      try {
        const capsRes = await apiFetch('/api/auth/capabilities');
        const caps = (await capsRes.json()) as CapabilitiesResponse;
        nextDesktopMode = Boolean(caps.desktopMode);
        if (!cancelled) {
          setDesktopMode(nextDesktopMode);
        }
      } catch {
        if (!cancelled) {
          setDesktopMode(false);
        }
      }

      try {
        const res = await apiFetch('/api/auth/me');
        const data = (await res.json()) as MeResponse;
        if (!cancelled) {
          setCsrfToken(data.csrfToken);
          setUser(data.user);
        }
        return;
      } catch {
        /* fall through to desktop session */
      }

      if (nextDesktopMode) {
        try {
          const res = await apiFetch('/api/auth/desktop-session', {
            method: 'POST',
          });
          const data = (await res.json()) as MeResponse;
          if (!cancelled) {
            setCsrfToken(data.csrfToken);
            setUser(data.user);
          }
          return;
        } catch {
          if (!cancelled) {
            setUser(null);
          }
        }
      } else if (!cancelled) {
        setUser(null);
      }
    }

    void loadSession().finally(() => {
      if (!cancelled) {
        setLoading(false);
      }
    });

    return () => {
      cancelled = true;
    };
  }, []);

  const establishSession = useCallback((nextUser: User, token: string) => {
    setCsrfToken(token);
    setUser(nextUser);
  }, []);

  const logout = useCallback(async () => {
    try {
      await apiFetch('/api/auth/logout', { method: 'POST' });
    } finally {
      setCsrfToken('');
      setUser(null);
    }
  }, []);

  return (
    <SessionContext.Provider
      value={{ user, loading, desktopMode, establishSession, logout }}
    >
      {children}
    </SessionContext.Provider>
  );
}
