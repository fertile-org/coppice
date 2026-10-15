export function SkipToBoard() {
  return (
    <a
      href="#board"
      className="keyboard-focus sr-only focus:not-sr-only focus:fixed focus:left-3 focus:top-3 focus:z-50 focus:rounded-md focus:border focus:border-border focus:bg-surface focus:px-3 focus:py-2 focus:font-body focus:text-sm focus:text-text-primary"
      onClick={(event) => {
        const target = document.querySelector('[data-board-tabstop="true"]');
        if (!(target instanceof HTMLElement)) return;
        event.preventDefault();
        target.focus();
      }}
    >
      Skip to board
    </a>
  );
}
