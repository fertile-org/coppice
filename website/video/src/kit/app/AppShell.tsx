import type { LucideIcon } from "lucide-react";
import {
  Bell,
  BookOpen,
  Bot,
  ChartColumn,
  ChevronRight,
  CircleHelp,
  FolderGit2,
  LayoutGrid,
  MessageSquare,
  Moon,
  PanelLeftClose,
  Puzzle,
  Search,
  Settings,
  Wrench,
} from "lucide-react";
import React from "react";
import { AbsoluteFill, Img, staticFile } from "remotion";
import "../tokens";

/**
 * App layout size, then scaled to fill VIDEO (1440×900).
 * 1280×800 ×1.125 keeps kit Tailwind sizes (designed around a 1280-wide window)
 * while matching the marketing 16:10 canvas.
 */
export const APP_LOGICAL = { width: 1280, height: 800, scale: 1.125 } as const;

// Mirrors web/src/components/AppShell.tsx for an admin in the desktop app
// (Users hidden, System and Help groups closed), dark theme. `active` is the nav item
// the route highlights (NavLink isActive); Boards by default.
// Nav order matches current AppShell: Boards, Chat, Agents, Knowledge, …
// Top bar: desktop mode spacer, ThemeToggle (dark), NotificationBell (no unread).
// Search: web/src/features/keyboard/SidebarSearch.tsx with the non-macOS hint.
// Not drawn: the `coppice-grain` texture (0.012 opacity in dark, invisible at video size).
const NAV: Array<{ label: string; icon: LucideIcon; group?: boolean }> = [
  { label: "Boards", icon: LayoutGrid },
  { label: "Chat", icon: MessageSquare },
  { label: "Agents", icon: Bot },
  { label: "Knowledge", icon: BookOpen },
  { label: "Repositories", icon: FolderGit2 },
  { label: "Plugins", icon: Puzzle },
  { label: "Usage", icon: ChartColumn },
  { label: "System", icon: Wrench, group: true },
  { label: "Settings", icon: Settings },
  { label: "Help", icon: CircleHelp, group: true },
];

const LINK = "flex items-center rounded-md font-body text-sm gap-3 px-3 py-2";
const ICON = "h-[1.125rem] w-[1.125rem] shrink-0";

export type NavItem = "Boards" | "Agents" | "Chat" | "Knowledge" | "Repositories" | "Plugins" | "Usage" | "Settings";

export const AppShell: React.FC<{ children?: React.ReactNode; active?: NavItem }> = ({
  children,
  active = "Boards",
}) => (
  <AbsoluteFill className="bg-background">
    <div
      className="relative flex overflow-hidden bg-background"
      style={{
        width: APP_LOGICAL.width,
        height: APP_LOGICAL.height,
        scale: String(APP_LOGICAL.scale),
        transformOrigin: "0 0",
      }}
    >
      <aside className="flex w-56 shrink-0 flex-col border-r border-border bg-surface">
        <div className="flex h-14 shrink-0 items-center gap-3 border-b border-border px-4">
          <Img src={staticFile("logo-dark@2x.webp")} className="h-8 w-8 shrink-0" />
          <span className="font-display text-lg font-semibold tracking-tight text-text-primary">
            Coppice
          </span>
        </div>

        <div className="shrink-0 px-2 pt-2">
          <div
            className="flex w-full min-w-0 items-center gap-2 rounded-md border px-2.5 py-1.5 font-body text-sm"
            style={{
              borderColor: "var(--color-field-border)",
              backgroundColor: "var(--color-field-bg)",
              boxShadow: "inset 0 1px 2px rgba(0, 0, 0, 0.06)",
            }}
          >
            <Search className={`${ICON} text-text-muted`} aria-hidden />
            <span className="min-w-0 flex-1 truncate text-text-muted">Search…</span>
            <span className="shrink-0 font-mono text-xs text-text-muted">Ctrl+K</span>
          </div>
        </div>

        <nav className="flex min-h-0 flex-1 flex-col gap-0.5 p-2">
          {NAV.map(({ label, icon: Icon, group }) => (
            <div
              key={label}
              className={`${LINK} ${label === active ? "bg-accent-muted text-accent" : "text-text-secondary"}`}
            >
              <Icon className={ICON} aria-hidden />
              <span className={group ? "flex-1 text-left" : ""}>{label}</span>
              {group && <ChevronRight className="h-4 w-4 shrink-0" aria-hidden />}
            </div>
          ))}
        </nav>

        <div className="shrink-0 border-t border-border p-2">
          <div className={`${LINK} w-full text-text-secondary`}>
            <PanelLeftClose className={ICON} />
            <span>Collapse</span>
          </div>
        </div>
      </aside>

      <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        <header className="flex h-14 shrink-0 items-center justify-end gap-4 border-b border-border bg-surface px-6">
          <span className="mr-auto" />
          <div className="inline-flex items-center gap-2 rounded-md px-3 py-1.5 font-body text-sm text-text-secondary">
            <Moon className="h-4 w-4 shrink-0" aria-hidden />
            <span className="capitalize">dark</span>
          </div>
          <div className="relative flex h-11 w-11 items-center justify-center rounded-full border border-transparent text-text-secondary">
            <Bell aria-hidden="true" className="h-5 w-5" strokeWidth={1.8} />
          </div>
        </header>

        {/* Not positioned: an absolute child (the ticket drawer) covers the whole window. */}
        <main className="min-h-0 w-full flex-1 overflow-hidden px-8 py-8">{children}</main>
      </div>
    </div>
  </AbsoluteFill>
);
