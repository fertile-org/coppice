import { useQuery } from '@tanstack/react-query';
import hljs from 'highlight.js/lib/core';
import bash from 'highlight.js/lib/languages/bash';
import css from 'highlight.js/lib/languages/css';
import go from 'highlight.js/lib/languages/go';
import ini from 'highlight.js/lib/languages/ini';
import javascript from 'highlight.js/lib/languages/javascript';
import json from 'highlight.js/lib/languages/json';
import markdown from 'highlight.js/lib/languages/markdown';
import python from 'highlight.js/lib/languages/python';
import rust from 'highlight.js/lib/languages/rust';
import sql from 'highlight.js/lib/languages/sql';
import typescript from 'highlight.js/lib/languages/typescript';
import xml from 'highlight.js/lib/languages/xml';
import yaml from 'highlight.js/lib/languages/yaml';
import { ExternalLink, X } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { Button } from '../../components/ui/button';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '../../components/ui/dialog';
import { ApiError, apiFetch } from '../../lib/api';
import { formatFileSize } from '../../lib/attachments';
import {
  formatFileReference,
  highlightLanguage,
  ticketFileUrl,
  type FileReference,
} from './fileReference';

hljs.registerLanguage('bash', bash);
hljs.registerLanguage('css', css);
hljs.registerLanguage('go', go);
hljs.registerLanguage('ini', ini);
hljs.registerLanguage('javascript', javascript);
hljs.registerLanguage('json', json);
hljs.registerLanguage('markdown', markdown);
hljs.registerLanguage('python', python);
hljs.registerLanguage('rust', rust);
hljs.registerLanguage('sql', sql);
hljs.registerLanguage('typescript', typescript);
hljs.registerLanguage('xml', xml);
hljs.registerLanguage('yaml', yaml);

/** Highlighting large files freezes the dialog; past this they show as plain text. */
const MAX_HIGHLIGHT_BYTES = 200 * 1024;
/** Matches the `leading-5` line height of the gutter and code columns. */
const LINE_HEIGHT_PX = 20;

type FileSource = 'worktree' | 'repo';

interface TextFile {
  text: string;
  source: FileSource | null;
  resolvedPath: string | null;
}

function sourceFromHeader(value: string | null): FileSource | null {
  return value === 'worktree' || value === 'repo' ? value : null;
}

function decodeResolvedPath(value: string | null): string | null {
  if (!value) return null;
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

const SOURCE_LABEL: Record<FileSource, string> = {
  worktree: 'Ticket worktree',
  repo: 'Repository checkout',
};

function filePreviewErrorMessage(err: unknown): string {
  const status = err instanceof ApiError ? err.status : 0;
  switch (status) {
    case 409:
      return 'This path matches several files, use the full path.';
    case 404:
      return 'This file was not found in the ticket worktree or the repository checkout.';
    case 413:
      return 'This file is too large to preview.';
    case 415:
      return 'Preview is not available for this file type.';
    case 400:
      return 'This path cannot be previewed. The repository may not be ready.';
    default:
      return 'Unable to load this file.';
  }
}

function useTextFile(ticketId: string, path: string, enabled: boolean) {
  return useQuery({
    queryKey: ['ticket-file', ticketId, path],
    enabled,
    retry: false,
    queryFn: async (): Promise<TextFile> => {
      const res = await apiFetch(ticketFileUrl(ticketId, path));
      return {
        text: await res.text(),
        source: sourceFromHeader(res.headers.get('x-coppice-file-source')),
        resolvedPath: decodeResolvedPath(res.headers.get('x-coppice-file-path')),
      };
    },
  });
}

function TextPreview({ file, text }: { file: FileReference; text: string }) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const lines = useMemo(() => text.replace(/\n$/, '').split('\n'), [text]);
  const html = useMemo(() => {
    const language = highlightLanguage(file.path);
    if (!language || text.length > MAX_HIGHLIGHT_BYTES || !hljs.getLanguage(language)) {
      return null;
    }
    return hljs.highlight(text, { language }).value;
  }, [file.path, text]);
  const first = file.line && file.line <= lines.length ? file.line : undefined;
  const last = first ? Math.min(file.endLine ?? first, lines.length) : undefined;

  useEffect(() => {
    if (!first || !scrollRef.current) return;
    scrollRef.current.scrollTop = Math.max(0, (first - 4) * LINE_HEIGHT_PX);
  }, [first]);

  return (
    <div
      ref={scrollRef}
      data-testid="file-preview-text"
      className="file-preview min-h-0 flex-1 overflow-auto bg-surface-raised"
    >
      <div className="relative flex min-w-max py-2 font-mono text-xs leading-5">
        {first && last && (
          <div
            aria-hidden
            data-testid="file-preview-highlight"
            className="pointer-events-none absolute inset-x-0 bg-warning-muted"
            style={{
              top: 8 + (first - 1) * LINE_HEIGHT_PX,
              height: (last - first + 1) * LINE_HEIGHT_PX,
            }}
          />
        )}
        <pre
          aria-hidden
          className="relative select-none border-r border-border pl-4 pr-3 text-right text-text-muted"
        >
          {lines.map((_, index) => `${index + 1}\n`).join('')}
        </pre>
        {html !== null ? (
          <pre className="relative pl-4 pr-6 text-text-primary">
            <code className="hljs" dangerouslySetInnerHTML={{ __html: html }} />
          </pre>
        ) : (
          <pre className="relative pl-4 pr-6 text-text-primary">
            <code>{text}</code>
          </pre>
        )}
      </div>
    </div>
  );
}

function ImagePreview({
  file,
  url,
  onFailed,
}: {
  file: FileReference;
  url: string;
  onFailed: (failed: boolean) => void;
}) {
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    onFailed(failed);
  }, [failed, onFailed]);
  if (failed) {
    return <PreviewMessage>Unable to load this image.</PreviewMessage>;
  }
  return (
    <div className="flex min-h-0 flex-1 items-center justify-center overflow-auto bg-paper-100 p-4">
      <img
        src={url}
        alt={file.path}
        onError={() => setFailed(true)}
        className="max-h-[70vh] max-w-full rounded-md object-contain shadow-md"
      />
    </div>
  );
}

function PreviewMessage({ children }: { children: string }) {
  return (
    <div className="flex min-h-[160px] flex-1 items-center justify-center bg-paper-100 p-6">
      <p className="max-w-sm text-center font-body text-sm text-text-secondary">{children}</p>
    </div>
  );
}

interface FilePreviewModalProps {
  /** Keep the modal mounted and toggle this so the exit animation plays. */
  open: boolean;
  ticketId: string;
  file: FileReference;
  onClose: () => void;
}

export function FilePreviewModal({ open, ticketId, file, onClose }: FilePreviewModalProps) {
  const url = ticketFileUrl(ticketId, file.path);
  const isText = file.kind === 'text';
  const textFile = useTextFile(ticketId, file.path, open && isText);
  const [imageFailed, setImageFailed] = useState(false);
  useEffect(() => {
    setImageFailed(false);
  }, [url]);
  const source = textFile.data?.source;
  const titleFile = textFile.data?.resolvedPath
    ? { ...file, path: textFile.data.resolvedPath }
    : file;
  const showOpenRaw = isText ? !textFile.isError : !imageFailed;
  const details = [
    source ? SOURCE_LABEL[source] : null,
    textFile.data ? formatFileSize(new Blob([textFile.data.text]).size) : null,
  ].filter(Boolean);

  return (
    <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogContent
        className="z-[110] flex max-h-[90vh] max-w-5xl flex-col overflow-hidden bg-surface-raised p-0 shadow-2xl"
        overlayClassName="z-[110] bg-overlay-strong backdrop-blur-[2px]"
      >
        <header className="flex shrink-0 items-center justify-between gap-3 border-b border-border px-4 py-3">
          <div className="min-w-0">
            <DialogTitle className="truncate font-mono text-sm font-medium text-text-primary">
              {formatFileReference(titleFile)}
            </DialogTitle>
            <DialogDescription className="text-xs text-text-muted">
              {details.length > 0 ? details.join(' · ') : 'File preview'}
            </DialogDescription>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            {showOpenRaw ? (
              <Button variant="secondary" size="sm" asChild>
                <a href={url} target="_blank" rel="noopener noreferrer">
                  <ExternalLink className="size-3.5" />
                  Open raw
                </a>
              </Button>
            ) : null}
            <DialogClose
              className="rounded-md border border-border p-1.5 text-text-secondary transition-colors duration-fast hover:text-text-primary"
              aria-label="Close preview"
            >
              <X className="size-4" />
            </DialogClose>
          </div>
        </header>

        {!isText ? (
          <ImagePreview key={url} file={file} url={url} onFailed={setImageFailed} />
        ) : textFile.isError ? (
          <PreviewMessage>{filePreviewErrorMessage(textFile.error)}</PreviewMessage>
        ) : textFile.data ? (
          <TextPreview file={file} text={textFile.data.text} />
        ) : (
          <PreviewMessage>Loading…</PreviewMessage>
        )}
      </DialogContent>
    </Dialog>
  );
}
