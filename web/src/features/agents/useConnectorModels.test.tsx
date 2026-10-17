import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react';
import { useState, type ReactNode } from 'react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { AgentForm, type AgentFormValues } from './AgentForm';
import { useConnectorModels } from './useAgents';

const apiFetch = vi.hoisted(() => vi.fn());

vi.mock('../../lib/api', () => ({ apiFetch }));

vi.mock('../plugins/usePlugins', () => ({
  usePlugins: () => ({ data: [], isLoading: false }),
}));

afterEach(() => {
  apiFetch.mockReset();
});

function client() {
  return new QueryClient({
    defaultOptions: { queries: { staleTime: 30_000, retry: false } },
  });
}

function wrapper(queryClient: QueryClient) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  };
}

function jsonResponse(body: unknown) {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  });
}

function baseValues(overrides: Partial<AgentFormValues> = {}): AgentFormValues {
  return {
    name: 'Builder',
    role: 'Developer',
    responsibilities: '',
    systemPrompt: '',
    connector: 'cursor',
    modelProvider: '',
    model: '',
    enabled: true,
    pluginIds: [],
    ...overrides,
  };
}

describe('useConnectorModels refresh', () => {
  it('bypasses the 30s cache and requests refresh=true', async () => {
    apiFetch.mockImplementation(async (path: string) => {
      if (path.includes('/models')) {
        return jsonResponse({ items: [{ id: 'auto', name: 'Auto' }] });
      }
      return jsonResponse({ items: [{ id: 'cursor' }] });
    });
    const queryClient = client();
    const { result } = renderHook(() => useConnectorModels('cursor'), {
      wrapper: wrapper(queryClient),
    });

    await waitFor(() => expect(result.current.groups[0]?.models[0]?.id).toBe('auto'));
    const callsAfterLoad = apiFetch.mock.calls.length;

    await act(async () => {
      await result.current.refresh();
    });

    const later = apiFetch.mock.calls.slice(callsAfterLoad).map((call) => String(call[0]));
    expect(later).toContain(
      '/api/connectors/cursor/model-providers/cursor/models?refresh=true',
    );
  });

  it('refetches model providers when none are loaded, then requests models', async () => {
    let providers: { id: string }[] = [];
    apiFetch.mockImplementation(async (path: string) => {
      if (path.includes('/models')) {
        return jsonResponse({
          items: [
            { id: 'auto', name: 'Auto' },
            { id: 'composer-2.5', name: 'Composer 2.5' },
          ],
        });
      }
      return jsonResponse({ items: providers });
    });
    const queryClient = client();
    const { result } = renderHook(() => useConnectorModels('cursor'), {
      wrapper: wrapper(queryClient),
    });

    await waitFor(() => expect(result.current.isLoading).toBe(false));
    expect(result.current.groups).toEqual([]);
    expect(result.current.isError).toBe(true);
    providers = [{ id: 'cursor' }];

    await act(async () => {
      await result.current.refresh();
    });

    const paths = apiFetch.mock.calls.map((call) => String(call[0]));
    expect(paths).toContain('/api/connectors/cursor/model-providers?refresh=true');
    expect(paths).toContain(
      '/api/connectors/cursor/model-providers/cursor/models?refresh=true',
    );
    await waitFor(() => expect(result.current.groups[0]?.models.map((model) => model.id)).toEqual([
      'auto',
      'composer-2.5',
    ]));
  });

  it('sends a provider refresh and ignores a second click', async () => {
    let releaseProviders: (response: Response) => void = () => {};
    const gated = new Promise<Response>((resolve) => {
      releaseProviders = resolve;
    });
    apiFetch.mockImplementation((path: string) => {
      const url = String(path);
      if (url.includes('/model-providers?refresh=true')) return gated;
      if (url.includes('/models')) {
        return Promise.resolve(jsonResponse({ items: [{ id: 'auto', name: 'Auto' }] }));
      }
      return Promise.resolve(jsonResponse({ items: [{ id: 'cursor' }] }));
    });
    const queryClient = client();

    function Harness() {
      const [values, setValues] = useState(baseValues());
      return (
        <MemoryRouter>
          <QueryClientProvider client={queryClient}>
            <AgentForm
              mode="edit"
              values={values}
              onChange={setValues}
              onSubmit={() => {}}
              onCancel={() => {}}
              connectorOptions={[{ id: 'cursor', displayName: 'Cursor' }]}
            />
          </QueryClientProvider>
        </MemoryRouter>
      );
    }

    render(<Harness />);
    const button = await screen.findByRole('button', { name: 'Refresh models' });
    await waitFor(() => expect(button).not.toBeDisabled());

    fireEvent.click(button);
    fireEvent.click(button);

    await act(async () => {
      releaseProviders(
        jsonResponse({ items: [{ id: 'cursor' }, { id: 'anthropic' }] }),
      );
      await gated;
    });

    const paths = apiFetch.mock.calls.map((call) => String(call[0]));
    expect(paths.filter((path) => path.includes('/model-providers?refresh=true'))).toEqual([
      '/api/connectors/cursor/model-providers?refresh=true',
    ]);
    expect(paths).toContain(
      '/api/connectors/cursor/model-providers/cursor/models?refresh=true',
    );
    expect(paths).toContain(
      '/api/connectors/cursor/model-providers/anthropic/models?refresh=true',
    );
  });

  it('shows a loading state on Refresh models while the refetch is in flight', async () => {
    let releaseModels: (response: Response) => void = () => {};
    const gated = new Promise<Response>((resolve) => {
      releaseModels = resolve;
    });
    apiFetch.mockImplementation((path: string) => {
      if (String(path).includes('refresh=true')) return gated;
      if (String(path).includes('/models')) {
        return Promise.resolve(jsonResponse({ items: [{ id: 'auto', name: 'Auto' }] }));
      }
      return Promise.resolve(jsonResponse({ items: [{ id: 'cursor' }] }));
    });
    const queryClient = client();

    function Harness() {
      const [values, setValues] = useState(baseValues());
      return (
        <MemoryRouter>
          <QueryClientProvider client={queryClient}>
            <AgentForm
              mode="edit"
              values={values}
              onChange={setValues}
              onSubmit={() => {}}
              onCancel={() => {}}
              connectorOptions={[{ id: 'cursor', displayName: 'Cursor' }]}
            />
          </QueryClientProvider>
        </MemoryRouter>
      );
    }

    render(<Harness />);
    const button = await screen.findByRole('button', { name: 'Refresh models' });
    await waitFor(() => expect(button).not.toBeDisabled());

    fireEvent.click(button);
    await waitFor(() => expect(button).toHaveAttribute('aria-busy', 'true'));
    expect(button).toBeDisabled();

    await act(async () => {
      releaseModels(jsonResponse({ items: [{ id: 'auto', name: 'Auto' }] }));
      await gated;
    });
    await waitFor(() => expect(button).not.toHaveAttribute('aria-busy'));
  });
});

describe('empty model list', () => {
  it('shows the failure copy when the server returns no models', async () => {
    apiFetch.mockImplementation(async (path: string) => {
      if (path.includes('/models')) return jsonResponse({ items: [] });
      return jsonResponse({ items: [{ id: 'cursor' }] });
    });
    const queryClient = client();

    function Harness() {
      const [values, setValues] = useState(baseValues());
      return (
        <MemoryRouter>
          <QueryClientProvider client={queryClient}>
            <AgentForm
              mode="create"
              values={values}
              onChange={setValues}
              onSubmit={() => {}}
              onCancel={() => {}}
              connectorOptions={[{ id: 'cursor', displayName: 'Cursor' }]}
            />
          </QueryClientProvider>
        </MemoryRouter>
      );
    }

    render(<Harness />);
    expect(
      await screen.findByText(
        "Couldn't get Cursor's models. Using Cursor's default for now.",
      ),
    ).toBeInTheDocument();
  });
});
