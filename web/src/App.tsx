import { BrowserRouter, Navigate, Route, Routes } from 'react-router-dom';
import { QueryClientProvider } from '@tanstack/react-query';
import { AppShell } from './components/AppShell';
import { ToastProvider } from './components/ToastProvider';
import { ProtectedRoute } from './components/ProtectedRoute';
import { AgentsPage } from './features/agents/AgentsPage';
import { AuthProvider } from './features/auth/AuthProvider';
import { LoginPage } from './features/auth/LoginPage';
import { BoardPage } from './features/board/BoardPage';
import { ProjectPickerPage } from './features/projects/ProjectPickerPage';
import { RepositoriesPage } from './features/repos/RepositoriesPage';
import { UsersPage } from './features/users/UsersPage';
import { queryClient } from './lib/query-client';

function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <ToastProvider>
      <BrowserRouter>
        <AuthProvider>
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
