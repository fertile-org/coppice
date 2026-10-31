import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../lib/api';
import { useChooseDesign } from './useDesigns';

const apiFetch = vi.hoisted(() => vi.fn());

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch,
}));

function renderChoose() {
  const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
  const invalidate = vi.spyOn(client, 'invalidateQueries');
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  const { result } = renderHook(() => useChooseDesign('t1'), { wrapper });
  return { result, invalidate };
}

describe('useChooseDesign', () => {
  beforeEach(() => {
    apiFetch.mockReset();
  });

  it('refreshes the designs after a saved pick', async () => {
    apiFetch.mockResolvedValue(new Response(null, { status: 204 }));
    const { result, invalidate } = renderChoose();
    await result.current.mutateAsync({ setId: 's1', optionId: 'b' });
    await waitFor(() =>
      expect(invalidate).toHaveBeenCalledWith({ queryKey: ['designs', 'ticket', 't1'] }),
    );
  });

  it('refreshes the designs after a refused pick, so the footer catches up', async () => {
    apiFetch.mockRejectedValue(new ApiError(400, JSON.stringify({ message: 'No.' })));
    const { result, invalidate } = renderChoose();
    await expect(result.current.mutateAsync({ setId: 's1', optionId: 'b' })).rejects.toThrow();
    await waitFor(() =>
      expect(invalidate).toHaveBeenCalledWith({ queryKey: ['designs', 'ticket', 't1'] }),
    );
  });
});
