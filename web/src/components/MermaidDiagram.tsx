import { useEffect, useState } from 'react';

const DIAGRAM_SOURCE_HINT =
  "This diagram couldn't be drawn, so here's its source.";

type RenderState =
  | { status: 'pending' }
  | { status: 'drawn'; svg: string }
  | { status: 'failed' };

function diagramTheme(): 'dark' | 'neutral' {
  return document.documentElement.dataset.theme === 'dark' ? 'dark' : 'neutral';
}

function useDiagramTheme(): 'dark' | 'neutral' {
  const [theme, setTheme] = useState(diagramTheme);

  useEffect(() => {
    const root = document.documentElement;
    const observer = new MutationObserver(() => {
      setTheme(diagramTheme());
    });
    observer.observe(root, {
      attributes: true,
      attributeFilter: ['data-theme'],
    });
    return () => observer.disconnect();
  }, []);

  return theme;
}

/**
 * Draws one fenced mermaid diagram. The library loads only when this mounts.
 */
export function MermaidDiagram({ source }: { source: string }) {
  const diagram = source.trim();
  const theme = useDiagramTheme();
  return <MermaidSvg key={`${theme}:${diagram}`} diagram={diagram} theme={theme} />;
}

function MermaidSvg({
  diagram,
  theme,
}: {
  diagram: string;
  theme: 'dark' | 'neutral';
}) {
  const [state, setState] = useState<RenderState>({ status: 'pending' });

  useEffect(() => {
    let cancelled = false;
    const id = `coppice-mermaid-${crypto.randomUUID()}`;

    void (async () => {
      try {
        const { default: mermaid } = await import('mermaid');
        mermaid.initialize({
          startOnLoad: false,
          securityLevel: 'strict',
          htmlLabels: false,
          flowchart: { htmlLabels: false },
          suppressErrorRendering: true,
          theme,
          fontFamily: 'Roboto, system-ui, sans-serif',
        });
        const { svg } = await mermaid.render(id, diagram);
        if (!cancelled) setState({ status: 'drawn', svg });
      } catch {
        if (!cancelled) setState({ status: 'failed' });
      } finally {
        document.getElementById(`d${id}`)?.remove();
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [diagram, theme]);

  if (state.status === 'failed') {
    return (
      <div className="mb-3">
        <p className="mb-2 font-body text-sm leading-relaxed text-text-secondary">
          {DIAGRAM_SOURCE_HINT}
        </p>
        <pre className="overflow-x-auto rounded-md border border-border bg-paper-200 p-3 font-mono text-xs leading-relaxed">
          <code>{diagram}</code>
        </pre>
      </div>
    );
  }

  return (
    <div
      className="mb-3 min-h-24 overflow-x-auto [&_svg]:h-auto [&_svg]:max-w-full"
      aria-busy={state.status === 'pending' ? true : undefined}
    >
      {state.status === 'drawn' ? (
        <div dangerouslySetInnerHTML={{ __html: state.svg }} />
      ) : null}
    </div>
  );
}
