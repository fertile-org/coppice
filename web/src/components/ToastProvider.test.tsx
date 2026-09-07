import '@testing-library/jest-dom/vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ToastProvider, useToast } from './ToastProvider';

function ToastHarness({
  onMount,
}: {
  onMount: (toast: ReturnType<typeof useToast>) => void;
}) {
  const toast = useToast();
  onMount(toast);
  return null;
}

function renderWithToast(onMount: (toast: ReturnType<typeof useToast>) => void) {
  return render(
    <ToastProvider>
      <ToastHarness onMount={onMount} />
    </ToastProvider>,
  );
}

describe('ToastProvider', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('dismiss control removes the toast without invoking onClick', () => {
    const onClick = vi.fn();
    let api: ReturnType<typeof useToast> | null = null;

    renderWithToast((toast) => {
      api = toast;
    });

    act(() => {
      api!.error('Agent run failed', { persistent: true, onClick });
    });

    expect(screen.getByText('Agent run failed')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Dismiss' }));

    expect(screen.queryByText('Agent run failed')).not.toBeInTheDocument();
    expect(onClick).not.toHaveBeenCalled();
  });

  it('clickable message invokes onClick and dismisses the toast', () => {
    const onClick = vi.fn();
    let api: ReturnType<typeof useToast> | null = null;

    renderWithToast((toast) => {
      api = toast;
    });

    act(() => {
      api!.error('Open ticket', { persistent: true, onClick });
    });

    fireEvent.click(screen.getByRole('button', { name: 'Open ticket' }));

    expect(onClick).toHaveBeenCalledTimes(1);
    expect(screen.queryByText('Open ticket')).not.toBeInTheDocument();
  });

  it('shows progress only for non-persistent toasts', () => {
    let api: ReturnType<typeof useToast> | null = null;

    renderWithToast((toast) => {
      api = toast;
    });

    act(() => {
      api!.success('Saved');
    });
    expect(screen.getByTestId('toast-progress')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Dismiss' }));

    act(() => {
      api!.error('Still failing', { persistent: true });
    });
    expect(screen.queryByTestId('toast-progress')).not.toBeInTheDocument();
  });

  it('auto-dismisses non-persistent toasts after 3000ms', () => {
    let api: ReturnType<typeof useToast> | null = null;

    renderWithToast((toast) => {
      api = toast;
    });

    act(() => {
      api!.success('Done');
    });
    expect(screen.getByText('Done')).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(2999);
    });
    expect(screen.getByText('Done')).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(screen.queryByText('Done')).not.toBeInTheDocument();
  });
});
