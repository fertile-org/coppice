import { ListFilter } from 'lucide-react';
import { Button } from '../../components/ui/button';
import { Input } from '../../components/ui/input';
import { Label } from '../../components/ui/label';
import { RepoDrawer } from '../repos/RepoDrawer';
import { BOARD_COLUMNS, type TicketStatus } from './columns';
import type {
  ActivityPreset,
  BoardFilters,
  TicketVisibility,
} from './boardFilters';
import { boardFiltersAreDefault } from './boardFilters';
import { cn } from '../../lib/utils';

const VISIBILITY_OPTIONS: { value: TicketVisibility; label: string }[] = [
  { value: 'active', label: 'Active only' },
  { value: 'include_archived', label: 'Include archived' },
  { value: 'archived', label: 'Archived only' },
];

const ACTIVITY_OPTIONS: { value: ActivityPreset; label: string }[] = [
  { value: 'any', label: 'Any time' },
  { value: '7d', label: 'Last 7 days' },
  { value: '30d', label: 'Last 30 days' },
  { value: '90d', label: 'Last 90 days' },
  { value: 'custom', label: 'Custom range' },
];

export interface BoardFilterDrawerProps {
  open: boolean;
  filters: BoardFilters;
  onChange: (filters: BoardFilters) => void;
  onClose: () => void;
  onClear: () => void;
}

export function BoardFilterDrawer({
  open,
  filters,
  onChange,
  onClose,
  onClear,
}: BoardFilterDrawerProps) {
  function toggleStatus(status: TicketStatus) {
    const has = filters.statuses.includes(status);
    onChange({
      ...filters,
      statuses: has
        ? filters.statuses.filter((s) => s !== status)
        : [...filters.statuses, status],
    });
  }

  return (
    <RepoDrawer
      open={open}
      ariaLabel="Board filters"
      title="Filters"
      description="Narrow which tickets appear on the board."
      onClose={onClose}
    >
      <div className="flex h-full flex-col gap-6">
        <div className="space-y-2">
          <Label htmlFor="board-filter-search">Search</Label>
          <Input
            id="board-filter-search"
            type="search"
            value={filters.q}
            onChange={(e) => onChange({ ...filters, q: e.target.value })}
            placeholder="Filter by title…"
            autoFocus
          />
        </div>

        <fieldset className="space-y-2">
          <legend className="font-body text-sm font-medium text-text-secondary">
            Visibility
          </legend>
          <div className="flex flex-col gap-1.5">
            {VISIBILITY_OPTIONS.map((option) => (
              <label
                key={option.value}
                className="flex cursor-pointer items-center gap-2 font-body text-sm text-text-primary"
              >
                <input
                  type="radio"
                  name="board-filter-visibility"
                  value={option.value}
                  checked={filters.visibility === option.value}
                  onChange={() =>
                    onChange({ ...filters, visibility: option.value })
                  }
                  className="h-4 w-4 border-border text-moss-600 focus:ring-moss-500"
                />
                {option.label}
              </label>
            ))}
          </div>
        </fieldset>

        <fieldset className="space-y-2">
          <legend className="font-body text-sm font-medium text-text-secondary">
            Status
          </legend>
          <p className="font-body text-xs text-text-muted">
            Leave empty to show all columns. Selected statuses filter cards only.
          </p>
          <div className="grid grid-cols-1 gap-1.5 sm:grid-cols-2">
            {BOARD_COLUMNS.map((column) => {
              const checked = filters.statuses.includes(column.status);
              return (
                <label
                  key={column.status}
                  className={cn(
                    'flex cursor-pointer items-center gap-2 rounded-md border px-2.5 py-2 font-body text-sm transition-colors duration-fast',
                    checked
                      ? 'border-moss-500 bg-moss-600/10 text-text-primary'
                      : 'border-border text-text-secondary hover:border-border-strong hover:text-text-primary',
                  )}
                >
                  <input
                    type="checkbox"
                    checked={checked}
                    onChange={() => toggleStatus(column.status)}
                    className="h-4 w-4 rounded border-border text-moss-600 focus:ring-moss-500"
                  />
                  {column.label}
                </label>
              );
            })}
          </div>
        </fieldset>

        <fieldset className="space-y-2">
          <legend className="font-body text-sm font-medium text-text-secondary">
            Last activity
          </legend>
          <div className="flex flex-col gap-1.5">
            {ACTIVITY_OPTIONS.map((option) => (
              <label
                key={option.value}
                className="flex cursor-pointer items-center gap-2 font-body text-sm text-text-primary"
              >
                <input
                  type="radio"
                  name="board-filter-activity"
                  value={option.value}
                  checked={filters.activityPreset === option.value}
                  onChange={() =>
                    onChange({
                      ...filters,
                      activityPreset: option.value,
                      activitySince:
                        option.value === 'custom'
                          ? filters.activitySince
                          : null,
                      activityUntil:
                        option.value === 'custom'
                          ? filters.activityUntil
                          : null,
                    })
                  }
                  className="h-4 w-4 border-border text-moss-600 focus:ring-moss-500"
                />
                {option.label}
              </label>
            ))}
          </div>
          {filters.activityPreset === 'custom' && (
            <div className="mt-2 grid grid-cols-1 gap-3 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label htmlFor="board-filter-since">From</Label>
                <Input
                  id="board-filter-since"
                  type="date"
                  value={filters.activitySince ?? ''}
                  onChange={(e) =>
                    onChange({
                      ...filters,
                      activitySince: e.target.value || null,
                    })
                  }
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="board-filter-until">To</Label>
                <Input
                  id="board-filter-until"
                  type="date"
                  value={filters.activityUntil ?? ''}
                  onChange={(e) =>
                    onChange({
                      ...filters,
                      activityUntil: e.target.value || null,
                    })
                  }
                />
              </div>
            </div>
          )}
        </fieldset>

        <div className="mt-auto flex items-center justify-between gap-3 border-t border-border pt-4">
          <Button
            type="button"
            variant="ghost"
            onClick={onClear}
            disabled={boardFiltersAreDefault(filters)}
          >
            Clear all
          </Button>
          <Button type="button" variant="secondary" onClick={onClose}>
            Done
          </Button>
        </div>
      </div>
    </RepoDrawer>
  );
}

export interface BoardFilterButtonProps {
  activeCount: number;
  onClick: () => void;
}

export function BoardFilterButton({
  activeCount,
  onClick,
}: BoardFilterButtonProps) {
  return (
    <Button
      type="button"
      variant="secondary"
      onClick={onClick}
      aria-label={
        activeCount > 0 ? `Filters, ${activeCount} active` : 'Filters'
      }
    >
      <ListFilter className="size-4" aria-hidden="true" />
      Filter
      {activeCount > 0 ? (
        <span className="inline-flex min-w-5 items-center justify-center rounded-md bg-moss-600 px-1.5 py-0.5 font-body text-xs font-medium text-paper-50">
          {activeCount}
        </span>
      ) : null}
    </Button>
  );
}
