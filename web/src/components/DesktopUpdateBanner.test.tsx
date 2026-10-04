import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DesktopUpdateBanner } from './DesktopUpdateBanner';

function stubBridge(info: { version: string; url: string } | null) {
  window.coppiceDesktop = {
    pickDirectory: vi.fn(async () => null),
    appInfo: vi.fn(async () => ({ version: '1.0.0', platform: 'linux', arch: 'x64' })),
    getUpdateInfo: vi.fn(async () => info),
  };
}

describe('DesktopUpdateBanner', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    delete window.coppiceDesktop;
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

  it('shows the available version with a download link', async () => {
    stubBridge({ version: '9.0.0', url: 'https://x' });
    render(<DesktopUpdateBanner />);
    expect(await screen.findByText('Coppice 9.0.0 is available')).toBeInTheDocument();
    const link = screen.getByRole('link', { name: 'Download' });
    expect(link).toHaveAttribute('href', 'https://x');
    expect(link).toHaveAttribute('target', '_blank');
  });

  it('dismisses and stays hidden for the same version', async () => {
    stubBridge({ version: '9.0.0', url: 'https://x' });
    const { unmount } = render(<DesktopUpdateBanner />);
    fireEvent.click(await screen.findByRole('button', { name: 'Dismiss update notice' }));
    expect(screen.queryByText('Coppice 9.0.0 is available')).not.toBeInTheDocument();
    expect(localStorage.getItem('coppice.dismissedUpdate')).toBe('9.0.0');
    unmount();

    const { container } = render(<DesktopUpdateBanner />);
    await vi.waitFor(() => expect(window.coppiceDesktop?.getUpdateInfo).toHaveBeenCalledTimes(2));
    expect(container).toBeEmptyDOMElement();
  });

  it('shows again when a newer version than the dismissed one appears', async () => {
    localStorage.setItem('coppice.dismissedUpdate', '8.0.0');
    stubBridge({ version: '9.0.0', url: 'https://x' });
    render(<DesktopUpdateBanner />);
    expect(await screen.findByText('Coppice 9.0.0 is available')).toBeInTheDocument();
  });
});
