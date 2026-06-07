function App() {
  return (
    <div className="coppice-grain min-h-screen bg-background">
      <header className="border-b border-border bg-surface px-8 py-4">
        <div className="mx-auto flex max-w-6xl items-center gap-3">
          <span
            className="inline-block h-3 w-3 rounded-full bg-accent"
            aria-hidden="true"
          />
          <h1 className="font-display text-2xl font-semibold tracking-tight text-text-primary">
            Coppice
          </h1>
          <span className="font-body text-sm text-text-secondary">
            grow an agent team from shared roots
          </span>
        </div>
      </header>

      <main className="mx-auto max-w-6xl px-8 py-12">
        <p className="max-w-xl font-body text-lg leading-relaxed text-text-secondary">
          Agent workspace scaffold ready. Board, tickets, and agents ship in M02.
        </p>
      </main>
    </div>
  )
}

export default App
