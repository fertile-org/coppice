import { useState, type FormEvent } from 'react';
import { Navigate } from 'react-router-dom';
import { ApiError } from '../../lib/api';
import { createUserSchema } from '../../lib/schemas/agent';
import { useSession } from '../auth/useSession';
import { useCreateUser, useUsers } from './useUsers';

function formatDate(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  return date.toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
  });
}

function CreateUserForm() {
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const createUser = useCreateUser();

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    const parsed = createUserSchema.safeParse({ email, password });
    if (!parsed.success) {
      setError(parsed.error.issues[0]?.message ?? 'Invalid input.');
      return;
    }

    setError(null);
    try {
      await createUser.mutateAsync(parsed.data);
      setEmail('');
      setPassword('');
    } catch (err) {
      if (err instanceof ApiError && err.status === 409) {
        setError('That email is already registered.');
      } else if (err instanceof ApiError && err.status === 403) {
        setError('You do not have permission to create users.');
      } else {
        setError('Unable to create user. Please try again.');
      }
    }
  }

  return (
    <form
      onSubmit={(e) => void handleSubmit(e)}
      className="rounded-xl border border-border bg-surface-raised p-5 shadow-card"
    >
      <h2 className="font-display text-lg font-semibold text-bark-900">
        Add member
      </h2>
      <p className="mt-1 font-body text-sm text-text-secondary">
        New users are created with the member role.
      </p>

      <div className="mt-4 space-y-3">
        <div>
          <label
            htmlFor="user-email"
            className="mb-1 block font-body text-sm font-medium text-bark-800"
          >
            Email
          </label>
          <input
            id="user-email"
            type="email"
            required
            autoComplete="off"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            className="w-full rounded-md border border-border bg-paper-50 px-3 py-2 font-body text-sm text-text-primary outline-none transition-colors duration-fast focus:border-moss-500 focus:ring-2 focus:ring-moss-100"
          />
        </div>

        <div>
          <label
            htmlFor="user-password"
            className="mb-1 block font-body text-sm font-medium text-bark-800"
          >
            Password
          </label>
          <input
            id="user-password"
            type="password"
            required
            autoComplete="new-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            className="w-full rounded-md border border-border bg-paper-50 px-3 py-2 font-body text-sm text-text-primary outline-none transition-colors duration-fast focus:border-moss-500 focus:ring-2 focus:ring-moss-100"
          />
        </div>
      </div>

      {error && (
        <p
          role="alert"
          className="mt-3 rounded-md bg-danger-muted px-3 py-2 font-body text-sm text-danger"
        >
          {error}
        </p>
      )}

      <button
        type="submit"
        disabled={createUser.isPending}
        className="mt-4 rounded-md bg-moss-600 px-4 py-2 font-body text-sm font-medium text-paper-50 transition-colors duration-fast hover:bg-moss-700 disabled:opacity-60"
      >
        {createUser.isPending ? 'Creating…' : 'Create user'}
      </button>
    </form>
  );
}

export function UsersPage() {
  const { user, loading } = useSession();
  const { data: users, isLoading, isError, refetch } = useUsers();

  if (loading) {
    return (
      <p className="font-body text-sm text-text-muted">Loading session…</p>
    );
  }

  if (user?.role !== 'admin') {
    return <Navigate to="/projects" replace />;
  }

  return (
    <div>
      <div>
        <h1 className="font-display text-2xl font-semibold text-bark-900">
          Users
        </h1>
        <p className="mt-2 max-w-xl font-body text-text-secondary">
          Manage workspace members and roles.
        </p>
      </div>

      <div className="mt-8 grid gap-8 lg:grid-cols-[minmax(0,1fr)_320px]">
        <div>
          {isLoading && (
            <p className="font-body text-sm text-text-muted">Loading users…</p>
          )}

          {isError && (
            <div className="rounded-lg border border-danger-muted bg-danger-muted/50 p-4">
              <p className="font-body text-sm text-danger">
                Unable to load users.
              </p>
              <button
                type="button"
                onClick={() => void refetch()}
                className="mt-2 font-body text-sm font-medium text-moss-700 underline-offset-2 hover:underline"
              >
                Try again
              </button>
            </div>
          )}

          {!isLoading && !isError && users && (
            <div className="overflow-hidden rounded-xl border border-border bg-surface-raised shadow-card">
              <table className="w-full text-left">
                <thead>
                  <tr className="border-b border-border bg-paper-100">
                    <th className="px-4 py-3 font-body text-xs font-medium uppercase tracking-wide text-text-muted">
                      Email
                    </th>
                    <th className="px-4 py-3 font-body text-xs font-medium uppercase tracking-wide text-text-muted">
                      Role
                    </th>
                    <th className="px-4 py-3 font-body text-xs font-medium uppercase tracking-wide text-text-muted">
                      Joined
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {users.map((member) => (
                    <tr
                      key={member.id}
                      className="border-b border-border last:border-b-0"
                    >
                      <td className="px-4 py-3 font-body text-sm text-text-primary">
                        {member.email}
                        {member.id === user?.id && (
                          <span className="ml-2 font-body text-xs text-text-muted">
                            (you)
                          </span>
                        )}
                      </td>
                      <td className="px-4 py-3">
                        <span
                          className={[
                            'inline-flex rounded-full px-2 py-0.5 font-body text-xs font-medium capitalize',
                            member.role === 'admin'
                              ? 'bg-moss-100 text-moss-800'
                              : 'bg-paper-200 text-bark-700',
                          ].join(' ')}
                        >
                          {member.role}
                        </span>
                      </td>
                      <td className="px-4 py-3 font-body text-xs text-text-muted">
                        {formatDate(member.createdAt)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>

        <CreateUserForm />
      </div>
    </div>
  );
}
