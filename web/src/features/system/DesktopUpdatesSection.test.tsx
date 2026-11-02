import '@testing-library/jest-dom/vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CoppiceDesktopBridge, DesktopUpdateCheck, DesktopUpdateInfo } from '../../lib/desktop';
import { DesktopUpdatesSection } from './DesktopUpdatesSection';

const APT_COMMAND = 'sudo apt update && sudo apt install --only-upgrade coppice-desktop';
const LINUX_GUIDE =
  'Open Software Updater and install the Coppice update, or run this command in a terminal. It updates only Coppice.';

function shell(
  info: { platform: string; macInPlaceUpdates: boolean },
  extra?: Partial<CoppiceDesktopBridge>,
) {
  window.coppiceDesktop = {
    pickDirectory: vi.fn(async () => null),
    platform: info.platform,
    appInfo: vi.fn(async () => ({ version: '1.0.0', arch: 'arm64', ...info })),
    getUpdateSettings: vi.fn(async () => ({ automaticUpdates: true })),
    setUpdateSettings: vi.fn(async (next: { automaticUpdates: boolean }) => next),
    checkForUpdates: vi.fn(async () => ({ status: 'up-to-date' as const })),
    ...extra,
  };
}

function listenForUpdateInfo(): {
  push: (info: DesktopUpdateInfo | null) => void;
  extra: Pick<CoppiceDesktopBridge, 'onUpdateInfo'>;
} {
  let listener: ((info: DesktopUpdateInfo | null) => void) | undefined;
  return {
    push: (info) => listener?.(info),
    extra: {
      onUpdateInfo: (next) => {
        listener = next;
        return () => {
          listener = undefined;
        };
      },
    },
  };
}

function stubActiveRun(active: boolean) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => new Response(JSON.stringify({ active }), { status: 200 })),
  );
}

describe('DesktopUpdatesSection', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.unstubAllGlobals();
  });

  afterEach(() => {
    delete window.coppiceDesktop;
    vi.unstubAllGlobals();
    localStorage.clear();
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

  it('shows the apt guide on the page after Check for updates, with no modal', async () => {
    const { push, extra } = listenForUpdateInfo();
    const writeText = vi.fn(async () => undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    shell(
      { platform: 'linux', macInPlaceUpdates: false },
      {
        ...extra,
        getUpdateInfo: vi.fn(async () => null),
        checkForUpdates: vi.fn(async () => {
          push({ kind: 'linux', version: '9.0.0' });
          return { status: 'available' as const, version: '9.0.0' };
        }),
      },
    );

    render(<DesktopUpdatesSection />);
    fireEvent.click(await screen.findByRole('button', { name: 'Check for updates' }));

    expect(await screen.findByRole('heading', { name: 'Update to Coppice 9.0.0' })).toBeInTheDocument();
    expect(screen.getByText(LINUX_GUIDE)).toBeInTheDocument();
    expect(screen.getByText(APT_COMMAND)).toBeInTheDocument();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Copy command' }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(APT_COMMAND));
  });

  it('still shows the guide after that version was dismissed and while a run is active', async () => {
    localStorage.setItem('coppice.dismissedUpdate', '9.0.0');
    stubActiveRun(true);
    shell(
      { platform: 'linux', macInPlaceUpdates: false },
      {
        getUpdateInfo: vi.fn(async () => ({ kind: 'linux' as const, version: '9.0.0' })),
      },
    );

    render(<DesktopUpdatesSection />);
    expect(await screen.findByRole('heading', { name: 'Update to Coppice 9.0.0' })).toBeInTheDocument();
    expect(screen.getByText(APT_COMMAND)).toBeInTheDocument();
  });

  it('shows a message instead of restarting while a run is active', async () => {
    stubActiveRun(true);
    shell(
      { platform: 'linux', macInPlaceUpdates: false },
      {
        getUpdateInfo: vi.fn(async () => ({ kind: 'ready' as const, version: '9.0.0' })),
        restartToUpdate: vi.fn(async () => undefined),
      },
    );

    render(<DesktopUpdatesSection />);
    fireEvent.click(await screen.findByRole('button', { name: 'Restart Coppice' }));

    expect(
      await screen.findByText('Wait for the agent run to finish, then try again.'),
    ).toBeInTheDocument();
    expect(window.coppiceDesktop?.restartToUpdate).not.toHaveBeenCalled();
  });

  it('shows a message instead of downloading while a run is active', async () => {
    stubActiveRun(true);
    const open = vi.fn();
    vi.stubGlobal('open', open);
    shell(
      { platform: 'darwin', macInPlaceUpdates: false },
      {
        getUpdateInfo: vi.fn(async () => ({
          kind: 'download' as const,
          version: '9.0.0',
          url: 'https://example/release',
        })),
      },
    );

    render(<DesktopUpdatesSection />);
    fireEvent.click(await screen.findByRole('button', { name: 'Download update' }));

    expect(
      await screen.findByText('Wait for the agent run to finish, then try again.'),
    ).toBeInTheDocument();
    expect(open).not.toHaveBeenCalled();
  });

  it('shows the guide after leaving and reopening without another check', async () => {
    const getUpdateInfo = vi.fn(async () => ({ kind: 'linux' as const, version: '9.0.0' }));
    const checkForUpdates = vi.fn(async () => ({ status: 'up-to-date' as const }));
    shell(
      { platform: 'linux', macInPlaceUpdates: false },
      { getUpdateInfo, checkForUpdates },
    );

    const first = render(<DesktopUpdatesSection />);
    expect(await screen.findByRole('heading', { name: 'Update to Coppice 9.0.0' })).toBeInTheDocument();
    expect(checkForUpdates).not.toHaveBeenCalled();
    first.unmount();

    render(<DesktopUpdatesSection />);
    expect(await screen.findByRole('heading', { name: 'Update to Coppice 9.0.0' })).toBeInTheDocument();
    expect(checkForUpdates).not.toHaveBeenCalled();
  });

  it('switches to Restart Coppice once apt has installed the package', async () => {
    const { push, extra } = listenForUpdateInfo();
    shell(
      { platform: 'linux', macInPlaceUpdates: false },
      {
        ...extra,
        getUpdateInfo: vi.fn(async () => ({ kind: 'linux' as const, version: '9.0.0' })),
        restartToUpdate: vi.fn(async () => undefined),
      },
    );

    render(<DesktopUpdatesSection />);
    expect(await screen.findByText(APT_COMMAND)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Restart Coppice' })).not.toBeInTheDocument();

    await act(async () => {
      push({ kind: 'ready', version: '9.0.0' });
    });

    expect(
      await screen.findByText(
        'Coppice 9.0.0 is installed. Restart to start using it. Your boards and settings stay.',
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText(APT_COMMAND)).not.toBeInTheDocument();
    expect(screen.queryByText(LINUX_GUIDE)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Restart Coppice' }));
    await waitFor(() => expect(window.coppiceDesktop?.restartToUpdate).toHaveBeenCalled());
  });

  it('keeps the available text and hides the panel across nulls during a signed macOS download', async () => {
    const { push, extra } = listenForUpdateInfo();
    shell(
      { platform: 'darwin', macInPlaceUpdates: true },
      {
        ...extra,
        getUpdateInfo: vi.fn(async () => null),
        checkForUpdates: vi.fn(async () => ({ status: 'available' as const, version: '9.0.0' })),
      },
    );

    render(<DesktopUpdatesSection />);
    fireEvent.click(await screen.findByRole('button', { name: 'Check for updates' }));
    expect(await screen.findByText('Coppice 9.0.0 is available.')).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Update to Coppice 9.0.0' })).not.toBeInTheDocument();

    await act(async () => {
      push(null);
    });
    expect(screen.getByText('Coppice 9.0.0 is available.')).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Update to Coppice 9.0.0' })).not.toBeInTheDocument();
  });

  it('clears the panel and available text when update info goes from present to null', async () => {
    const { push, extra } = listenForUpdateInfo();
    shell(
      { platform: 'linux', macInPlaceUpdates: false },
      {
        ...extra,
        getUpdateInfo: vi.fn(async () => ({ kind: 'linux' as const, version: '9.0.0' })),
      },
    );

    render(<DesktopUpdatesSection />);
    expect(await screen.findByRole('heading', { name: 'Update to Coppice 9.0.0' })).toBeInTheDocument();

    await act(async () => {
      push(null);
    });
    expect(screen.queryByRole('heading', { name: 'Update to Coppice 9.0.0' })).not.toBeInTheDocument();
    expect(screen.queryByText('Coppice 9.0.0 is available.')).not.toBeInTheDocument();
  });

  it('does not let a late getUpdateInfo result overwrite a newer broadcast', async () => {
    const { push, extra } = listenForUpdateInfo();
    let resolveInfo: ((info: DesktopUpdateInfo | null) => void) | undefined;
    shell(
      { platform: 'linux', macInPlaceUpdates: false },
      {
        ...extra,
        getUpdateInfo: vi.fn(
          () =>
            new Promise<DesktopUpdateInfo | null>((resolve) => {
              resolveInfo = resolve;
            }),
        ),
      },
    );

    render(<DesktopUpdatesSection />);
    await screen.findByRole('button', { name: 'Check for updates' });
    await act(async () => {
      push({ kind: 'linux', version: '2.0.0' });
    });
    expect(await screen.findByRole('heading', { name: 'Update to Coppice 2.0.0' })).toBeInTheDocument();

    await act(async () => {
      resolveInfo?.(null);
    });
    expect(screen.getByRole('heading', { name: 'Update to Coppice 2.0.0' })).toBeInTheDocument();
  });

  it('offers Restart to update on signed macOS once the package is ready', async () => {
    shell(
      { platform: 'darwin', macInPlaceUpdates: true },
      {
        getUpdateInfo: vi.fn(async () => ({ kind: 'ready' as const, version: '9.0.0' })),
        restartToUpdate: vi.fn(async () => undefined),
      },
    );

    render(<DesktopUpdatesSection />);
    fireEvent.click(await screen.findByRole('button', { name: 'Restart to update' }));
    await waitFor(() => expect(window.coppiceDesktop?.restartToUpdate).toHaveBeenCalled());
    expect(screen.queryByText(APT_COMMAND)).not.toBeInTheDocument();
  });
});
