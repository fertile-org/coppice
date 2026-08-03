import { useCallback } from 'react';
import {
  BrowserRouter,
  Navigate,
  Route,
  Routes,
} from 'react-router-dom';
import { QueryClientProvider } from '@tanstack/react-query';
import { AppShell } from './components/AppShell';
import { ToastProvider, useToast } from './components/ToastProvider';
import { ProtectedRoute } from './components/ProtectedRoute';
import { AgentsPage } from './features/agents/AgentsPage';
import { AuthProvider } from './features/auth/AuthProvider';
import { useSession } from './features/auth/useSession';
import { LoginPage } from './features/auth/LoginPage';
import { BoardPage } from './features/board/BoardPage';
import { ProjectPickerPage } from './features/projects/ProjectPickerPage';
import { RepositoriesPage } from './features/repos/RepositoriesPage';
import { UsersPage } from './features/users/UsersPage';
import { CodeReviewPage } from './features/code/CodeReviewPage';
import { useOpenTicket } from './features/tickets/useOpenTicket';
import {
  useEventSocket,
  type AgentRunFinishedPayload,
} from './features/ws/useEventSocket';
import { queryClient } from './lib/query-client';

function EventSocketBridge() {
  const { user } = useSession();
  const toast = useToast();
  const openTicket = useOpenTicket();

  const handleRunFinished = useCallback(
    (payload: AgentRunFinishedPayload) => {
      if (payload.status === 'succeeded' || payload.status === 'blocked') {
        toast.success(`Agent run ${payload.status}`);
        return;
      }

      toast.error(`Agent run ${payload.status}`, {
        persistent: true,
        onClick: () => {
          void openTicket(payload.ticket_id);
        },
      });
    },
    [openTicket, toast],
  );

  useEventSocket({
    enabled: Boolean(user),
    onRunFinished: handleRunFinished,
  });

  return null;
}

function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <ToastProvider>
      <BrowserRouter>
        <AuthProvider>
          <EventSocketBridge />
          <Routes>
            <Route path="/login" element={<LoginPage />} />
            <Route element={<ProtectedRoute />}>
              <Route element={<AppShell />}>
                <Route path="/projects" element={<ProjectPickerPage />} />
                <Route
                  path="/projects/:projectId/board"
                  element={<BoardPage />}
                />
                <Route path="/agents" element={<AgentsPage />} />
                <Route
                  path="/settings/repositories"
                  element={<RepositoriesPage />}
                />
                <Route path="/settings/users" element={<UsersPage />} />
              </Route>
              <Route path="/code" element={<CodeReviewPage />} />
            </Route>
            <Route path="*" element={<Navigate to="/projects" replace />} />
          </Routes>
        </AuthProvider>
      </BrowserRouter>
      </ToastProvider>
    </QueryClientProvider>
  );
}

export default App;
