import { BookOpen, Lightbulb, Plus, Search, SearchX, ShieldCheck } from 'lucide-react';
import { useId, useMemo, useState, type ReactNode } from 'react';
import { Button } from '../../components/ui/button';
import { Combobox } from '../../components/ui/combobox';
import { Input } from '../../components/ui/input';
import type {
  KnowledgeItem,
  KnowledgeStatus,
  KnowledgeType,
} from '../../lib/schemas/knowledge';
import { useSession } from '../auth/useSession';
import { useBoards } from '../boards/useBoards';
import { useOpenTicket } from '../tickets/useOpenTicket';
import { CURATION_LITMUS, CURATION_TRUST_FRAMING } from './curationGuide';
import { CompactionStatusStrip } from './CompactionStatusStrip';
import { KnowledgeCard } from './KnowledgeCard';
import {
  KnowledgeFormDialog,
  RejectKnowledgeDialog,
  type KnowledgeFormMode,
} from './KnowledgeDialogs';
import { TYPE_LABELS, TYPE_OPTIONS, scopeLabel } from './knowledgeFormat';
import { useKnowledge } from './useKnowledge';

const STATUS_TABS: Array<{ value: KnowledgeStatus; label: string }> = [
  { value: 'pending', label: 'Pending' },
  { value: 'approved', label: 'Approved' },
  { value: 'rejected', label: 'Rejected' },
  { value: 'stale', label: 'Stale' },
];

const EMPTY_COPY: Record<KnowledgeStatus, string> = {
  pending: 'New manual and extracted candidates will wait here for review.',
  approved: 'Approved knowledge is what agents can retrieve during runs.',
  rejected: 'Rejected candidates stay here with their reason for the audit trail.',
  stale: 'Knowledge marked stale is kept here until it is superseded or re-approved.',
};

function matchesSearch(item: KnowledgeItem, terms: string[]): boolean {
  if (terms.length === 0) return true;
  const haystack = [
    item.title,
    item.content,
    TYPE_LABELS[item.knowledgeType],
    scopeLabel(item),
  ]
    .join(' ')
    .toLowerCase();
  return terms.every((term) => haystack.includes(term));
}

function PendingInboxGuidance() {
  return (
    <aside
      aria-label="Pending inbox guidance"
      className="flex gap-3 rounded-lg border border-moss-200 bg-moss-50 px-4 py-3"
    >
      <Lightbulb className="mt-0.5 size-4 shrink-0 text-moss-700" aria-hidden="true" />
      <div className="min-w-0">
        <p className="font-display text-sm font-semibold text-bark-900">{CURATION_LITMUS}</p>
        <p className="mt-1 font-body text-xs leading-relaxed text-text-secondary">
          {CURATION_TRUST_FRAMING} Use the approve/reject examples on each card to keep
          curation consistent.
        </p>
      </div>
    </aside>
  );
}

function EmptyState({
  icon,
  title,
  description,
  children,
}: {
  icon: ReactNode;
  title: string;
  description: string;
  children?: ReactNode;
}) {
  return (
    <div className="rounded-xl border border-dashed border-border bg-paper-100 px-8 py-12 text-center">
      <div className="mx-auto flex size-10 items-center justify-center rounded-full bg-moss-50 text-moss-600">
        {icon}
      </div>
      <h2 className="mt-3 font-display text-lg font-semibold text-bark-800">{title}</h2>
      <p className="mx-auto mt-1 max-w-md font-body text-sm text-text-secondary">
        {description}
      </p>
      {children && (
        <div className="mt-5 flex flex-wrap justify-center gap-2">{children}</div>
      )}
    </div>
  );
}

export function KnowledgePage() {
  const { user } = useSession();
  const canGovern = user?.role === 'admin';
  const { data: boards } = useBoards();
  const openTicket = useOpenTicket();
  const idPrefix = useId();
  const [status, setStatus] = useState<KnowledgeStatus>('pending');
  const [boardId, setBoardId] = useState('');
  const [knowledgeType, setKnowledgeType] = useState('');
  const [search, setSearch] = useState('');
  const [focusItemId, setFocusItemId] = useState<string | null>(null);
  const [formOpen, setFormOpen] = useState(false);
  const [formMode, setFormMode] = useState<KnowledgeFormMode>('create');
  const [formItem, setFormItem] = useState<KnowledgeItem | null>(null);
  const [rejectOpen, setRejectOpen] = useState(false);
  const [rejectItem, setRejectItem] = useState<KnowledgeItem | null>(null);

  const query = useKnowledge({
    status,
    boardId: boardId || undefined,
    knowledgeType: (knowledgeType || undefined) as KnowledgeType | undefined,
  });
  const items = useMemo(
    () => query.data?.pages.flatMap((page) => page.items) ?? [],
    [query.data],
  );
  const searchTerms = useMemo(
    () => search.trim().toLowerCase().split(/\s+/).filter(Boolean),
    [search],
  );
  const visibleItems = useMemo(
    () => items.filter((item) => matchesSearch(item, searchTerms)),
    [items, searchTerms],
  );
  const boardOptions = useMemo(
    () => boards?.map((board) => ({ value: board.id, label: board.name })) ?? [],
    [boards],
  );

  const hasFilters = searchTerms.length > 0 || boardId !== '' || knowledgeType !== '';
  const tabId = (value: KnowledgeStatus) => `${idPrefix}-tab-${value}`;
  const panelId = `${idPrefix}-panel`;

  function clearFilters() {
    setSearch('');
    setBoardId('');
    setKnowledgeType('');
  }

  function selectStatus(next: KnowledgeStatus) {
    setFocusItemId(null);
    setStatus(next);
  }

  function openNeighbor(neighborId: string) {
    clearFilters();
    setFocusItemId(neighborId);
    setStatus('approved');
  }

  function openForm(mode: KnowledgeFormMode, item: KnowledgeItem | null = null) {
    setFormMode(mode);
    setFormItem(item);
    setFormOpen(true);
  }

  function openReject(item: KnowledgeItem) {
    setRejectItem(item);
    setRejectOpen(true);
  }

  const addButton = canGovern ? (
    <Button type="button" onClick={() => openForm('create')}>
      <Plus className="size-4" aria-hidden="true" />
      Add knowledge
    </Button>
  ) : null;

  return (
    <div>
      <header className="flex flex-wrap items-start justify-between gap-4">
        <div className="min-w-0 max-w-2xl">
          <h1 className="font-display text-2xl font-semibold text-bark-900">Knowledge</h1>
          <p className="mt-2 font-body text-text-secondary">
            Review durable facts before agents can use them. Every revision keeps its
            source, policy decision, and run history.
          </p>
          <span
            title={CURATION_TRUST_FRAMING}
            className="mt-3 inline-flex items-center gap-1.5 rounded-full border border-moss-200 bg-moss-50 px-2.5 py-0.5 font-body text-xs font-medium text-moss-800"
          >
            <ShieldCheck className="size-3.5" aria-hidden="true" />
            Human-governed · fail-closed
          </span>
        </div>
        {canGovern ? (
          addButton
        ) : (
          <p className="flex max-w-xs items-start gap-2 rounded-lg border border-border bg-surface px-3 py-2 font-body text-xs text-text-secondary">
            <ShieldCheck className="mt-0.5 size-4 shrink-0 text-moss-600" aria-hidden="true" />
            Read-only. An administrator can create candidates and govern their lifecycle.
          </p>
        )}
      </header>

      <CompactionStatusStrip />

      <div
        role="tablist"
        aria-label="Knowledge status"
        className="mt-6 flex gap-1 overflow-x-auto border-b border-border"
      >
        {STATUS_TABS.map((tab) => (
          <button
            key={tab.value}
            id={tabId(tab.value)}
            type="button"
            role="tab"
            aria-selected={status === tab.value}
            aria-controls={panelId}
            onClick={() => selectStatus(tab.value)}
            className={[
              '-mb-px shrink-0 border-b-2 px-3 py-2 font-body text-sm transition-colors duration-fast',
              status === tab.value
                ? 'border-accent font-medium text-accent'
                : 'border-transparent text-text-secondary hover:text-text-primary',
            ].join(' ')}
          >
            {tab.label}
          </button>
        ))}
      </div>

      <section
        id={panelId}
        role="tabpanel"
        aria-labelledby={tabId(status)}
        className="mt-4 space-y-4"
      >
        <div className="flex flex-col gap-3 sm:flex-row sm:flex-wrap sm:items-center">
          <div className="relative min-w-0 flex-1 sm:min-w-64">
            <Search
              className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-text-muted"
              aria-hidden="true"
            />
            <Input
              type="search"
              aria-label="Search knowledge"
              placeholder="Search title, content, type, or scope…"
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              className="h-10 pl-9"
            />
          </div>
          <Combobox
            aria-label="Filter by board"
            className="sm:w-52"
            value={boardId}
            onValueChange={setBoardId}
            options={boardOptions}
            clearable
            clearLabel="All scopes"
            placeholder="All scopes"
            searchPlaceholder="Search boards…"
            emptyText="No boards found."
          />
          <Combobox
            aria-label="Filter by type"
            className="sm:w-52"
            value={knowledgeType}
            onValueChange={setKnowledgeType}
            options={TYPE_OPTIONS}
            clearable
            clearLabel="All types"
            placeholder="All types"
            searchPlaceholder="Search types…"
          />
          {hasFilters && (
            <Button type="button" variant="ghost" size="sm" onClick={clearFilters}>
              Clear filters
            </Button>
          )}
        </div>

        {status === 'pending' && <PendingInboxGuidance />}

        {query.isLoading && (
          <div className="rounded-xl border border-dashed border-border bg-paper-100 p-10 text-center">
            <BookOpen className="mx-auto size-6 text-text-muted" aria-hidden="true" />
            <p className="mt-2 font-body text-sm text-text-muted">Loading knowledge…</p>
          </div>
        )}

        {query.isError && (
          <div className="rounded-xl border border-danger-muted bg-danger-muted/30 p-5">
            <p className="font-body text-sm text-danger">Unable to load knowledge.</p>
            <Button
              type="button"
              variant="secondary"
              size="sm"
              className="mt-3"
              onClick={() => void query.refetch()}
            >
              Try again
            </Button>
          </div>
        )}

        {!query.isLoading && !query.isError && visibleItems.length === 0 &&
          (hasFilters ? (
            <EmptyState
              icon={<SearchX className="size-5" aria-hidden="true" />}
              title="No matching knowledge"
              description={
                query.hasNextPage
                  ? 'Nothing loaded so far matches your search. Load more or adjust the filters.'
                  : 'Nothing matches your search and filters on this tab.'
              }
            >
              <Button type="button" variant="secondary" onClick={clearFilters}>
                Clear filters
              </Button>
              {addButton}
            </EmptyState>
          ) : (
            <EmptyState
              icon={<BookOpen className="size-5" aria-hidden="true" />}
              title={`No ${status} knowledge`}
              description={EMPTY_COPY[status]}
            >
              {addButton}
            </EmptyState>
          ))}

        {visibleItems.length > 0 && (
          <div className="space-y-4">
            {visibleItems.map((item) => (
              <KnowledgeCard
                key={item.id}
                item={item}
                canGovern={canGovern}
                focused={focusItemId === item.id}
                onOpenTicket={openTicket}
                onOpenNeighbor={openNeighbor}
                onEdit={(target) => openForm('edit', target)}
                onSupersede={(target) => openForm('supersede', target)}
                onReject={openReject}
              />
            ))}
          </div>
        )}

        {query.hasNextPage && (
          <div className="flex justify-center">
            <Button
              type="button"
              variant="secondary"
              loading={query.isFetchingNextPage}
              onClick={() => void query.fetchNextPage()}
            >
              Load more
            </Button>
          </div>
        )}
      </section>

      <KnowledgeFormDialog
        open={formOpen}
        mode={formMode}
        item={formItem}
        onOpenChange={setFormOpen}
      />
      <RejectKnowledgeDialog
        open={rejectOpen}
        item={rejectItem}
        onOpenChange={setRejectOpen}
      />
    </div>
  );
}
