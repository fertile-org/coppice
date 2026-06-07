import { createContext, useContext } from 'react';

export interface User {
  id: string;
  email: string;
  role: string;
}

export interface SessionContextValue {
  user: User | null;
  loading: boolean;
  establishSession: (user: User, csrfToken: string) => void;
  logout: () => Promise<void>;
}

export const SessionContext = createContext<SessionContextValue | null>(null);

export function useSession(): SessionContextValue {
  const ctx = useContext(SessionContext);
  if (!ctx) {
    throw new Error('useSession must be used within AuthProvider');
  }
  return ctx;
}
