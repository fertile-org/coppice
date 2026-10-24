import '@testing-library/jest-dom/vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { DesktopUpdateCheck } from '../../lib/desktop';
import { DesktopUpdatesSection } from './DesktopUpdatesSection';

function shell(info: { platform: string; macInPlaceUpdates: boolean }) {
  window.coppiceDesktop = {
    pickDirectory: vi.fn(async () => null),
    platform: info.platform,
    appInfo: vi.fn(async () => ({ version: '1.0.0', arch: 'arm64', ...info })),
    getUpdateSettings: vi.fn(async () => ({ automaticUpdates: true })),
    setUpdateSettings: vi.fn(async (next: { automaticUpdates: boolean }) => next),
    checkForUpdates: vi.fn(async () => ({ status: 'up-to-date' as const })),
  };
}

describe('DesktopUpdatesSection', () => {
  afterEach(() => {
    delete window.coppiceDesktop;
  });

  it('describes a signed macOS build as a background download', async () => {
    shell({ platform: 'darwin', macInPlaceUpdates: true });
    render(<DesktopUpdatesSection />);
    expect(
      await screen.findByText('Download new versions in the background. They install when you restart Coppice.'),
    ).toBeInTheDocument();
  });

  it('describes an unsigned macOS build as a notice only', async () => {
    shell({ platform: 'darwin', macInPlaceUpdates: false });
    render(<DesktopUpdatesSection />);
    expect(await screen.findByText('Tell me when a new version is out.', { exact: true })).toBeInTheDocument();
    expect(screen.queryByText(/Updates install through apt/)).not.toBeInTheDocument();
    expect(screen.queryByText(/They install when you restart Coppice/)).not.toBeInTheDocument();
  });

  it('describes Linux updates as apt', async () => {
    shell({ platform: 'linux', macInPlaceUpdates: false });
    render(<DesktopUpdatesSection />);
    expect(
      await screen.findByText('Tell me when a new version is out. Updates install through apt.'),
    ).toBeInTheDocument();
  });

  it('stays hidden outside the desktop shell', () => {
    const { container } = render(<DesktopUpdatesSection />);
    expect(container).toBeEmptyDOMElement();
  });

  it('remembers the automatic updates switch', async () => {
    const store = { automaticUpdates: true };
    window.coppiceDesktop = {
      pickDirectory: vi.fn(async () => null),
      platform: 'linux',
      appInfo: vi.fn(async () => ({
        version: '0.1.0-rc.7',
        platform: 'linux',
        arch: 'x64',
        macInPlaceUpdates: false,
      })),
      getUpdateSettings: vi.fn(async () => ({ ...store })),
      setUpdateSettings: vi.fn(async (next: { automaticUpdates: boolean }) => {
        store.automaticUpdates = next.automaticUpdates;
        return { ...store };
      }),
      checkForUpdates: vi.fn(async () => ({ status: 'up-to-date' as const })),
    };

    const first = render(<DesktopUpdatesSection />);
    expect(await screen.findByRole('region', { name: 'Updates' })).toBeInTheDocument();
    expect(screen.getByText('Version 0.1.0-rc.7')).toBeInTheDocument();
    expect(screen.getByText('Tell me when a new version is out. Updates install through apt.')).toBeInTheDocument();
    const sw = screen.getByRole('switch', { name: 'Automatic updates' });
    expect(sw).toHaveAttribute('aria-checked', 'true');
    await act(async () => {
      fireEvent.click(sw);
    });
    await waitFor(() => {
      expect(store.automaticUpdates).toBe(false);
      expect(sw).toHaveAttribute('aria-checked', 'false');
    });
    first.unmount();

    render(<DesktopUpdatesSection />);
    expect(await screen.findByRole('switch', { name: 'Automatic updates' })).toHaveAttribute(
      'aria-checked',
      'false',
    );
  });

  it('reports each check result', async () => {
    const pending: Array<(value: DesktopUpdateCheck) => void> = [];
    window.coppiceDesktop = {
      pickDirectory: vi.fn(async () => null),
      platform: 'darwin',
      appInfo: vi.fn(async () => ({
        version: '1.0.0',
        platform: 'darwin',
        arch: 'arm64',
        macInPlaceUpdates: true,
      })),
      getUpdateSettings: vi.fn(async () => ({ automaticUpdates: true })),
      setUpdateSettings: vi.fn(async (next: { automaticUpdates: boolean }) => next),
      checkForUpdates: vi.fn(
        () =>
          new Promise<DesktopUpdateCheck>((resolve) => {
            pending.push(resolve);
          }),
      ),
    };

    render(<DesktopUpdatesSection />);
    expect(
      await screen.findByText('Download new versions in the background. They install when you restart Coppice.'),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Check for updates' }));
    expect(await screen.findByText('Checking…')).toBeInTheDocument();
    pending[0]({ status: 'up-to-date' });
    expect(await screen.findByText("You're up to date.")).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Check for updates' }));
    pending[1]({ status: 'available', version: '1.4.0-rc.2' });
    expect(await screen.findByText('Coppice 1.4.0-rc.2 is available.')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Check for updates' }));
    pending[2]({ status: 'error' });
    expect(await screen.findByText("Couldn't check for updates. Try again later.")).toBeInTheDocument();
  });
});
