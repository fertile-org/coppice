import { useCallback, useEffect, useState, type ReactNode } from 'react';
import { apiFetch, setCsrfToken } from '../../lib/api';
import { SessionContext, type User } from './useSession';

interface MeResponse {
  user: User;
  csrfToken: string;
}

interface AuthProviderProps {
  children: ReactNode;
}

export function AuthProvider({ children }: AuthProviderProps) {
  const [user, setUser] = useState<User | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;

    async function loadSession() {
      try {
        const res = await apiFetch('/api/auth/me');
        const data = (await res.json()) as MeResponse;
        if (!cancelled) {
          setCsrfToken(data.csrfToken);
          setUser(data.user);
        }
      } catch {
        if (!cancelled) {
          setUser(null);
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    }

    loadSession();
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
      value={{ user, loading, establishSession, logout }}
    >
      {children}
    </SessionContext.Provider>
  );
}
