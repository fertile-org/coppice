import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { IMPORT_NOTICE, ImportNoticeBanner } from './ImportNoticeBanner';

const apiFetch = vi.hoisted(() => vi.fn());

vi.mock('../lib/api', () => ({ apiFetch }));

function renderBanner() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <ImportNoticeBanner />
    </QueryClientProvider>,
  );
}

describe('ImportNoticeBanner', () => {
  it('shows the import notice and dismisses it', async () => {
    apiFetch.mockResolvedValue(
      new Response(JSON.stringify({ message: IMPORT_NOTICE })),
    );
    renderBanner();

    expect(await screen.findByTestId('import-notice')).toHaveTextContent(IMPORT_NOTICE);
    expect(IMPORT_NOTICE).toBe(
      'Imported your git author, worker, knowledge and connector settings from `~/.config/coppice/config.toml`. Connectors stay as set in Connectors. The old file is now `config.toml.imported`.',
    );

    fireEvent.click(screen.getByRole('button', { name: 'Dismiss' }));
    await waitFor(() => {
      expect(screen.queryByTestId('import-notice')).not.toBeInTheDocument();
    });
  });

  it('stays hidden when there is no notice', async () => {
    apiFetch.mockResolvedValue(new Response(JSON.stringify({ message: null })));
    renderBanner();
    await waitFor(() => expect(apiFetch).toHaveBeenCalledWith('/api/import-notice'));
    expect(screen.queryByTestId('import-notice')).not.toBeInTheDocument();
  });
});
