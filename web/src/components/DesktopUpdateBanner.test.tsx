import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DesktopUpdateInfo } from '../lib/desktop';
import { DesktopUpdateBanner } from './DesktopUpdateBanner';

function stubBridge(info: DesktopUpdateInfo | null) {
  window.coppiceDesktop = {
    pickDirectory: vi.fn(async () => null),
    appInfo: vi.fn(async () => ({ version: '1.0.0', platform: 'linux', arch: 'x64', macInPlaceUpdates: false })),
    getUpdateInfo: vi.fn(async () => info),
    restartToUpdate: vi.fn(async () => undefined),
  };
}

describe('DesktopUpdateBanner', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.unstubAllGlobals();
  });

  afterEach(() => {
    delete window.coppiceDesktop;
    vi.unstubAllGlobals();
  });

  it('renders nothing outside the desktop shell or without an update', async () => {
    const empty = render(<DesktopUpdateBanner />);
    expect(empty.container).toBeEmptyDOMElement();
    empty.unmount();

    stubBridge(null);
    const { container } = render(<DesktopUpdateBanner />);
    await vi.waitFor(() => expect(window.coppiceDesktop?.getUpdateInfo).toHaveBeenCalled());
    expect(container).toBeEmptyDOMElement();
  });

  it('shows Linux the apt command for Coppice in a dialog', async () => {
    stubBridge({ kind: 'linux', version: '9.0.0' });
    const writeText = vi.fn(async () => undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    render(<DesktopUpdateBanner />);
    const banner = await screen.findByRole('status');
    expect(banner).toHaveTextContent('Coppice 9.0.0 is available.');
    expect(banner).not.toHaveTextContent('sudo');
    expect(screen.getByRole('button', { name: 'Dismiss' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Download update' })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'How to update' }));
    const dialog = await screen.findByRole('dialog', { name: 'Update to Coppice 9.0.0' });
    expect(dialog).toHaveTextContent(
      'sudo apt update && sudo apt install --only-upgrade coppice-desktop',
    );
    fireEvent.click(screen.getByRole('button', { name: 'Copy command' }));
    await vi.waitFor(() =>
      expect(writeText).toHaveBeenCalledWith(
        'sudo apt update && sudo apt install --only-upgrade coppice-desktop',
      ),
    );
    expect(await screen.findByRole('button', { name: 'Copied' })).toBeInTheDocument();
  });

  it('turns the open Linux dialog into a restart once apt has installed', async () => {
    let push: ((info: DesktopUpdateInfo | null) => void) | undefined;
    stubBridge({ kind: 'linux', version: '9.0.0' });
    window.coppiceDesktop!.onUpdateInfo = (listener) => {
      push = listener;
      return () => {};
    };
    render(<DesktopUpdateBanner />);
    fireEvent.click(await screen.findByRole('button', { name: 'How to update' }));
    const dialog = await screen.findByRole('dialog', { name: 'Update to Coppice 9.0.0' });
    expect(within(dialog).queryByRole('button', { name: 'Restart Coppice' })).not.toBeInTheDocument();

    push?.({ kind: 'ready', version: '9.0.0' });
    fireEvent.click(await within(dialog).findByRole('button', { name: 'Restart Coppice' }));
    await vi.waitFor(() => expect(window.coppiceDesktop?.restartToUpdate).toHaveBeenCalled());
  });

  it('downloads unsigned macOS builds in the browser and restarts signed ones', async () => {
    stubBridge({ kind: 'download', version: '9.0.0', url: 'https://example/release' });
    const open = vi.fn();
    vi.stubGlobal('open', open);
    render(<DesktopUpdateBanner />);
    fireEvent.click(await screen.findByRole('button', { name: 'Download update' }));
    await vi.waitFor(() =>
      expect(open).toHaveBeenCalledWith('https://example/release', '_blank', 'noopener,noreferrer'),
    );
    cleanup();
    vi.unstubAllGlobals();

    stubBridge({ kind: 'ready', version: '9.0.0' });
    render(<DesktopUpdateBanner />);
    fireEvent.click(await screen.findByRole('button', { name: 'Restart to update' }));
    await vi.waitFor(() => expect(window.coppiceDesktop?.restartToUpdate).toHaveBeenCalled());
  });

  it('hides the banner while a run is active', async () => {
    stubBridge({ kind: 'linux', version: '9.0.0' });
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response(JSON.stringify({ active: true }), { status: 200 })),
    );
    const { container } = render(<DesktopUpdateBanner />);
    await vi.waitFor(() => expect(window.coppiceDesktop?.getUpdateInfo).toHaveBeenCalled());
    await vi.waitFor(() => expect(fetch).toHaveBeenCalled());
    expect(container).toBeEmptyDOMElement();
  });

});
