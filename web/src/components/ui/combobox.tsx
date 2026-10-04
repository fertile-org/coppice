import { Check, ChevronsUpDown } from 'lucide-react';
import * as React from 'react';
import { cn } from '../../lib/utils';
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from './command';
import { Popover, PopoverContent, PopoverTrigger } from './popover';

export type ComboboxOption = {
  value: string;
  label: string;
  description?: string;
  disabled?: boolean;
  group?: string;
  icon?: React.ReactNode;
};

export type ComboboxProps = {
  value: string | null | undefined;
  onValueChange: (value: string) => void;
  options: ComboboxOption[];
  placeholder?: string;
  searchPlaceholder?: string;
  emptyText?: string;
  disabled?: boolean;
  id?: string;
  name?: string;
  'aria-label'?: string;
  'aria-labelledby'?: string;
  'aria-describedby'?: string;
  'aria-invalid'?: boolean;
  'data-testid'?: string;
  className?: string;
  triggerClassName?: string;
  contentClassName?: string;
  /** Prepends an empty-value (`''`) item unless `options` already contains one. */
  clearable?: boolean;
  /** Label for the item added by `clearable`; defaults to `placeholder` or "None". */
  clearLabel?: string;
  /** `'auto'` shows the search input only when there are more than 5 options. */
  searchable?: boolean | 'auto';
  align?: 'start' | 'center' | 'end';
  /** Custom content for an option in the list and trigger; search still matches `label`. */
  renderLabel?: (option: ComboboxOption) => React.ReactNode;
  onBlur?: () => void;
  ref?: React.Ref<HTMLButtonElement>;
};

const AUTO_SEARCH_THRESHOLD = 5;

function itemKey(index: number) {
  return `combobox-option-${index}`;
}

function matchesSearch(_value: string, search: string, keywords?: string[]) {
  const terms = search.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (terms.length === 0) return 1;
  const haystack = (keywords ?? []).join(' ').toLowerCase();
  return terms.every((term) => haystack.includes(term)) ? 1 : 0;
}

function Combobox({
  value,
  onValueChange,
  options,
  placeholder = 'Select…',
  searchPlaceholder = 'Search…',
  emptyText = 'No results found.',
  disabled,
  id,
  name,
  'aria-label': ariaLabel,
  'aria-labelledby': ariaLabelledBy,
  'aria-describedby': ariaDescribedBy,
  'aria-invalid': ariaInvalid,
  'data-testid': testId,
  className,
  triggerClassName,
  contentClassName,
  clearable,
  clearLabel,
  searchable = 'auto',
  align = 'start',
  renderLabel,
  onBlur,
  ref,
}: ComboboxProps) {
  const [open, setOpen] = React.useState(false);
  const commandRef = React.useRef<HTMLDivElement>(null);
  const listRef = React.useRef<HTMLDivElement>(null);
  const current = value ?? '';

  const allOptions = React.useMemo(() => {
    if (!clearable || options.some((o) => o.value === '')) return options;
    return [{ value: '', label: clearLabel ?? (placeholder || 'None') }, ...options];
  }, [clearable, clearLabel, options, placeholder]);

  const groups = React.useMemo(() => {
    const byGroup = new Map<string, { option: ComboboxOption; index: number }[]>();
    allOptions.forEach((option, index) => {
      const key = option.group ?? '';
      const bucket = byGroup.get(key);
      if (bucket) bucket.push({ option, index });
      else byGroup.set(key, [{ option, index }]);
    });
    return [...byGroup.entries()];
  }, [allOptions]);

  const selectedIndex = allOptions.findIndex((o) => o.value === current);
  const selected = selectedIndex >= 0 ? allOptions[selectedIndex] : undefined;
  const showSearch =
    searchable === 'auto' ? allOptions.length > AUTO_SEARCH_THRESHOLD : searchable;

  React.useEffect(() => {
    if (!open) return;
    const frame = requestAnimationFrame(() => {
      listRef.current
        ?.querySelector('[aria-selected="true"]')
        ?.scrollIntoView?.({ block: 'nearest' });
    });
    return () => cancelAnimationFrame(frame);
  }, [open]);

  const handleOpenChange = (next: boolean) => {
    setOpen(next);
    if (!next) onBlur?.();
  };

  return (
    <div className={cn('relative w-full', className)}>
      <Popover open={open} onOpenChange={handleOpenChange} modal>
        <PopoverTrigger asChild>
          <button
            ref={ref}
            type="button"
            role="combobox"
            id={id}
            aria-expanded={open}
            aria-haspopup="listbox"
            aria-label={ariaLabel}
            aria-labelledby={ariaLabelledBy}
            aria-describedby={ariaDescribedBy}
            aria-invalid={ariaInvalid}
            data-testid={testId}
            data-placeholder={selected ? undefined : ''}
            disabled={disabled}
            className={cn(
              'field-control flex h-10 w-full items-center justify-between gap-2 px-3 py-2 text-left font-body text-sm',
              triggerClassName,
            )}
          >
            <span
              className={cn(
                'flex min-w-0 items-center gap-2 truncate',
                !selected && 'text-text-muted',
              )}
            >
              {selected?.icon}
              {selected && renderLabel ? (
                renderLabel(selected)
              ) : (
                <span className="truncate">{selected ? selected.label : placeholder}</span>
              )}
            </span>
            <ChevronsUpDown className="size-4 shrink-0 text-text-muted" aria-hidden />
          </button>
        </PopoverTrigger>
        <PopoverContent
          align={align}
          className={cn(
            'w-[var(--radix-popover-trigger-width)] min-w-[8rem] p-0',
            contentClassName,
          )}
          onOpenAutoFocus={(event) => {
            if (!showSearch) {
              event.preventDefault();
              commandRef.current?.focus();
            }
          }}
        >
          <Command
            ref={commandRef}
            filter={matchesSearch}
            defaultValue={selectedIndex >= 0 ? itemKey(selectedIndex) : undefined}
            loop
          >
            {showSearch ? <CommandInput placeholder={searchPlaceholder} /> : null}
            <CommandList ref={listRef}>
              <CommandEmpty>{emptyText}</CommandEmpty>
              {groups.map(([group, entries]) => (
                <CommandGroup key={group || '__ungrouped'} heading={group || undefined}>
                  {entries.map(({ option, index }) => {
                    const isSelected = index === selectedIndex;
                    return (
                      <CommandItem
                        key={itemKey(index)}
                        value={itemKey(index)}
                        keywords={[option.label, option.value, option.description ?? '']}
                        disabled={option.disabled}
                        aria-checked={isSelected}
                        onSelect={() => {
                          if (option.value !== current) onValueChange(option.value);
                          handleOpenChange(false);
                        }}
                      >
                        {option.icon}
                        <span className="flex min-w-0 flex-1 flex-col">
                          {renderLabel ? (
                            <span className="flex min-w-0">{renderLabel(option)}</span>
                          ) : (
                            <span className="truncate">{option.label}</span>
                          )}
                          {option.description ? (
                            <span className="truncate text-xs text-text-muted">
                              {option.description}
                            </span>
                          ) : null}
                        </span>
                        <Check
                          aria-hidden
                          className={cn(
                            'ml-auto size-4 shrink-0 text-accent',
                            isSelected ? 'opacity-100' : 'opacity-0',
                          )}
                        />
                      </CommandItem>
                    );
                  })}
                </CommandGroup>
              ))}
            </CommandList>
          </Command>
        </PopoverContent>
      </Popover>
      {name ? <input type="hidden" name={name} value={current} /> : null}
    </div>
  );
}

export { Combobox };
