/** The agent form's "Refresh models" control. Refetch is the caller's job. */
export function RefreshModelsButton({
  onClick,
  disabled = false,
}: {
  onClick: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-busy={disabled || undefined}
      className="font-body text-xs font-medium text-moss-700 underline-offset-2 hover:underline disabled:opacity-60"
    >
      Refresh models
    </button>
  );
}
