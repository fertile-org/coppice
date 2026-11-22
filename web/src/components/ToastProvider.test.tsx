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

  it('dismisses persistent toasts without firing onClick unless the body is clicked', () => {
    const onClick = vi.fn();
    let api: ReturnType<typeof useToast> | null = null;

    renderWithToast((toast) => {
      api = toast;
    });

    act(() => {
      api!.error('Agent run failed', { persistent: true, onClick });
    });
    fireEvent.click(screen.getByRole('button', { name: 'Dismiss' }));
    expect(onClick).not.toHaveBeenCalled();

    act(() => {
      api!.error('Open ticket', { persistent: true, onClick });
    });
    fireEvent.click(screen.getByRole('button', { name: 'Open ticket' }));
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it('auto-dismisses transient toasts and skips progress on persistent ones', () => {
    let api: ReturnType<typeof useToast> | null = null;

    renderWithToast((toast) => {
      api = toast;
    });

    act(() => {
      api!.success('Done');
    });
    expect(screen.getByTestId('toast-progress')).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(3000));
    expect(screen.queryByText('Done')).not.toBeInTheDocument();

    act(() => {
      api!.error('Still failing', { persistent: true });
    });
    expect(screen.queryByTestId('toast-progress')).not.toBeInTheDocument();
  });
});
