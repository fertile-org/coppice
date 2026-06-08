import {
  BOARD_COLUMNS,
  columnAccentClass,
  columnBgClass,
  columnBorderClass,
  type TicketStatus,
} from '../board/columns';
import { cn } from '../../lib/utils';

interface TicketStatusBadgeProps {
  status: TicketStatus;
  className?: string;
}

export function TicketStatusBadge({ status, className }: TicketStatusBadgeProps) {
  const column = BOARD_COLUMNS.find((entry) => entry.status === status);
  if (!column) return null;

  return (
    <span
      className={cn(
        'inline-flex items-center rounded-full border px-2.5 py-0.5 font-body text-xs font-semibold',
        columnBgClass(column.colorKey),
        columnBorderClass(column.colorKey),
        columnAccentClass(column.colorKey),
        className,
      )}
    >
      {column.label}
    </span>
  );
}
