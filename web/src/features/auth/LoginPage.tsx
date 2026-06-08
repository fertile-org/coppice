import { useState, type FormEvent } from 'react';
import { Navigate, useNavigate } from 'react-router-dom';
import { apiFetch, ApiError } from '../../lib/api';
import { useSession } from './useSession';

interface LoginResponse {
  user: { id: string; email: string; role: string };
  csrfToken: string;
}

export function LoginPage() {
  const { user, loading, establishSession } = useSession();
  const navigate = useNavigate();
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  if (!loading && user) {
    return <Navigate to="/projects" replace />;
  }

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setSubmitting(true);

    try {
      const res = await apiFetch('/api/auth/login', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email, password }),
      });
      const data = (await res.json()) as LoginResponse;
      establishSession(data.user, data.csrfToken);
      navigate('/projects');
    } catch (err) {
      if (err instanceof ApiError && err.status === 401) {
        setError('Invalid email or password.');
      } else {
        setError('Unable to sign in. Please try again.');
      }
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="coppice-grain flex min-h-screen items-center justify-center bg-background px-4">
      <div className="w-full max-w-md rounded-xl border border-border bg-surface-raised p-8 shadow-card">
        <div className="mb-8 flex flex-col items-center gap-4 text-center">
          <img
            src="/logo.webp"
            srcSet="/logo.webp 1x, /logo@2x.webp 2x"
            alt="Coppice"
            width={72}
            height={72}
            className="h-[4.5rem] w-[4.5rem] shrink-0"
          />
          <h1 className="font-display text-2xl font-semibold tracking-tight text-text-primary">
            Coppice
          </h1>
        </div>

        <p className="mb-6 font-body text-sm text-text-secondary">
          Sign in to your agent workspace.
        </p>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label
              htmlFor="email"
              className="mb-1 block font-body text-sm font-medium text-text-primary"
            >
              Email
            </label>
            <input
              id="email"
              type="email"
              autoComplete="email"
              required
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              className="field-control w-full px-3 py-2 font-body text-sm"
            />
          </div>

          <div>
            <label
              htmlFor="password"
              className="mb-1 block font-body text-sm font-medium text-text-primary"
            >
              Password
            </label>
            <input
              id="password"
              type="password"
              autoComplete="current-password"
              required
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className="field-control w-full px-3 py-2 font-body text-sm"
            />
          </div>

          {error && (
            <p
              role="alert"
              className="rounded-md bg-danger-muted px-3 py-2 font-body text-sm text-danger"
            >
              {error}
            </p>
          )}

          <button
            type="submit"
            disabled={submitting}
            className="w-full rounded-md bg-accent px-4 py-2 font-body text-sm font-medium text-accent-foreground transition-colors duration-fast hover:bg-accent-hover disabled:opacity-60"
          >
            {submitting ? 'Signing in…' : 'Sign in'}
          </button>
        </form>
      </div>
    </div>
  );
}
