import { Navigate, Outlet, useLocation } from 'react-router-dom';
import { useSession } from '../features/auth/useSession';

export function ProtectedRoute() {
  const { user, loading } = useSession();
  const location = useLocation();

  if (loading) {
    return (
      <div className="coppice-grain flex min-h-screen items-center justify-center bg-background">
        <p className="font-body text-sm text-text-secondary">Loading session…</p>
      </div>
    );
  }

  if (!user) {
    return <Navigate to="/login" state={{ from: location }} replace />;
  }

  return <Outlet />;
}
