import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
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

  it('renders nothing outside the desktop shell', () => {
    const { container } = render(<DesktopUpdateBanner />);
    expect(container).toBeEmptyDOMElement();
  });

  it('renders nothing when no update is available', async () => {
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

  it('offers a release-page download while macOS builds are unsigned', async () => {
    stubBridge({ kind: 'download', version: '9.0.0', url: 'https://example/release' });
    const open = vi.fn();
    vi.stubGlobal('open', open);
    render(<DesktopUpdateBanner />);
    expect(await screen.findByText('Coppice 9.0.0 is available.')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Download update' }));
    await vi.waitFor(() =>
      expect(open).toHaveBeenCalledWith('https://example/release', '_blank', 'noopener,noreferrer'),
    );
    expect(screen.getByRole('button', { name: 'Later' })).toBeInTheDocument();
  });

  it('offers restart once a signed macOS update has downloaded', async () => {
    stubBridge({ kind: 'ready', version: '9.0.0' });
    render(<DesktopUpdateBanner />);
    expect(await screen.findByText('Coppice 9.0.0 is ready.')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Restart to update' }));
    await vi.waitFor(() => expect(window.coppiceDesktop?.restartToUpdate).toHaveBeenCalled());
    expect(screen.getByRole('button', { name: 'Later' })).toBeInTheDocument();
  });

  it('dismisses and stays hidden for the same version', async () => {
    stubBridge({ kind: 'linux', version: '9.0.0' });
    const { unmount } = render(<DesktopUpdateBanner />);
    fireEvent.click(await screen.findByRole('button', { name: 'Dismiss' }));
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
    expect(localStorage.getItem('coppice.dismissedUpdate')).toBe('9.0.0');
    unmount();

    const { container } = render(<DesktopUpdateBanner />);
    await vi.waitFor(() => expect(window.coppiceDesktop?.getUpdateInfo).toHaveBeenCalledTimes(2));
    expect(container).toBeEmptyDOMElement();
  });

  it('shows again when a newer version than the dismissed one appears', async () => {
    localStorage.setItem('coppice.dismissedUpdate', '8.0.0');
    stubBridge({ kind: 'download', version: '9.0.0', url: 'https://example/release' });
    render(<DesktopUpdateBanner />);
    expect(await screen.findByText('Coppice 9.0.0 is available.')).toBeInTheDocument();
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

  it('shows a banner pushed after the first check', async () => {
    let push: ((info: DesktopUpdateInfo | null) => void) | undefined;
    window.coppiceDesktop = {
      pickDirectory: vi.fn(async () => null),
      getUpdateInfo: vi.fn(async () => null),
      onUpdateInfo: (listener) => {
        push = listener;
        return () => {};
      },
    };
    render(<DesktopUpdateBanner />);
    await vi.waitFor(() => expect(push).toBeTypeOf('function'));
    push?.({ kind: 'linux', version: '2.0.0' });
    expect(await screen.findByRole('button', { name: 'Dismiss' })).toBeInTheDocument();
  });
});
