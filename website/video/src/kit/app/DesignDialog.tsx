import { X } from "lucide-react";
import React from "react";
import { Button } from "./Button";
import { DARK_MODE_DESIGNS, DESIGN, type DesignOptionLabel } from "./copy";

// Mirrors web/src/features/designs/DesignDialog.tsx inside
// web/src/components/ui/dialog.tsx (DialogContent with showCloseButton), for a
// plan-run design set on a ticket in Plan Review (choice mode `update_plan`).
// Footer states shown: an unchosen option ("Choose X and update plan") and, once
// a pick has started the plan rewrite, `plan_running` ("The plan is being updated.").
//
// The preview is an iframe of the agent's own page in the app. That page keeps its
// own light colours in either app theme, so the preview is light while the dialog
// chrome around it stays in the kit's dark tokens. Layout, text and colours are the
// page source in e2e/screenshot/design-options.mjs (the set the app's screenshots
// use, static/screenshots/08-design-options.png). Sizes are the page's CSS px.
//
// Render inside <AppShell>, above <PlanPanel>: like the app's dialog it covers the
// drawer with its own overlay.

const P = DARK_MODE_DESIGNS.preview;

/**
 * The preview page's own colours, copied from the STYLE / extra CSS strings in
 * ../coppice/e2e/screenshot/design-options.mjs. Not app tokens: this is the agent's
 * page, not Coppice UI (kit README, Deviations).
 */
const PAGE = {
  body: "#f6f1e9", // body background
  text: "#292524", // body colour
  muted: "#78716c", // .lede, .hint
  panel: "#fffdf9", // .panel background
  panelBorder: "#e3dbcf", // .panel, .card border
  rowBorder: "#eee7dc", // .row + .row border-top
  value: "#57534e", // .value, .segments span
  accent: "#2f6b4f", // .switch, .segments .on, .card.on border
  onText: "#fff", // .segments .on colour, .switch knob
  segmentsBg: "#eee7dc", // .segments background
  shotLight: { bg: "#f6f1e9", i: "#a8a29e", b: "#ddd5c8" }, // .light
  shotDark: { bg: "#292524", i: "#a8a29e", b: "#57534e" }, // .dark
} as const;

const Row: React.FC<{ name: string; hint: string; children?: React.ReactNode; first?: boolean }> = ({
  name,
  hint,
  children,
  first = false,
}) => (
  <div
    className="flex items-center justify-between gap-[24px] px-[22px] py-[18px]"
    style={first ? undefined : { borderTop: `1px solid ${PAGE.rowBorder}` }}
  >
    <div>
      <div className="font-semibold">{name}</div>
      <div className="text-[14px]" style={{ color: PAGE.muted }}>
        {hint}
      </div>
    </div>
    {children}
  </div>
);

const ValueRow: React.FC<{ row: { name: string; hint: string; value: string } }> = ({ row }) => (
  <Row name={row.name} hint={row.hint}>
    <div style={{ color: PAGE.value }}>{row.value}</div>
  </Row>
);

const Shot: React.FC<{ light: boolean }> = ({ light }) => {
  const c = light ? PAGE.shotLight : PAGE.shotDark;
  return (
    <div
      className="grid h-[150px] content-start gap-[9px] rounded-[8px] p-[16px]"
      style={{ backgroundColor: c.bg }}
    >
      <i className="block h-[14px] w-[40%] rounded-[4px]" style={{ backgroundColor: c.i }} />
      {[0, 1, 2].map((n) => (
        <b key={n} className="block h-[10px] rounded-[4px]" style={{ backgroundColor: c.b }} />
      ))}
    </div>
  );
};

const Preview: React.FC<{ label: DesignOptionLabel }> = ({ label }) => (
  // The iframe: rounded-md border border-border (app chrome); the page fills it.
  <div className="h-full w-full overflow-hidden rounded-md border border-border" style={{ backgroundColor: PAGE.body }}>
    <div
      className="mx-auto max-w-[860px] px-[32px] py-[36px] font-body text-[15px] leading-[1.45]"
      style={{ color: PAGE.text }}
    >
      <h1 className="text-[28px] font-semibold tracking-[-0.02em]">{P.heading}</h1>
      <p className="mt-[4px]" style={{ color: PAGE.muted }}>
        {P.lede}
      </p>
      <section
        className="mt-[24px] rounded-[12px]"
        style={{ backgroundColor: PAGE.panel, border: `1px solid ${PAGE.panelBorder}` }}
      >
        {label === "A" && (
          <>
            <Row name={P.darkMode.name} hint={P.darkMode.hint} first>
              <div className="relative h-[30px] w-[52px] rounded-[15px]" style={{ backgroundColor: PAGE.accent }}>
                <div
                  className="absolute top-[3px] right-[3px] h-[24px] w-[24px] rounded-full"
                  style={{ backgroundColor: PAGE.onText }}
                />
              </div>
            </Row>
            <ValueRow row={P.language} />
            <ValueRow row={P.startPage} />
          </>
        )}
        {label === "B" && (
          <>
            <Row name={P.theme.name} hint={P.theme.hint} first>
              <div className="flex gap-[4px] rounded-[10px] p-[4px]" style={{ backgroundColor: PAGE.segmentsBg }}>
                {P.segments.map((s) => {
                  const on = s === "Dark";
                  return (
                    <span
                      key={s}
                      className="rounded-[7px] px-[20px] py-[7px] font-medium"
                      style={on ? { backgroundColor: PAGE.accent, color: PAGE.onText } : { color: PAGE.value }}
                    >
                      {s}
                    </span>
                  );
                })}
              </div>
            </Row>
            <ValueRow row={P.language} />
            <ValueRow row={P.startPage} />
          </>
        )}
        {label === "C" && (
          <>
            <Row name={P.themeCards.name} hint={P.themeCards.hint} first />
            <div className="grid grid-cols-2 gap-[18px] px-[22px] pb-[22px]">
              {P.cards.map((c) => {
                const on = c === "Dark";
                return (
                  <div
                    key={c}
                    className={`rounded-[12px] ${on ? "p-[9px]" : "p-[10px]"}`}
                    style={{ border: on ? `2px solid ${PAGE.accent}` : `1px solid ${PAGE.panelBorder}` }}
                  >
                    <Shot light={!on} />
                    <div className="mt-[8px] text-center font-semibold">{c}</div>
                  </div>
                );
              })}
            </div>
            <ValueRow row={P.language} />
          </>
        )}
      </section>
    </div>
  </div>
);

export const DesignDialog: React.FC<{
  /** The tab on screen. */
  active: DesignOptionLabel;
  /** The option the user picked, which gets the Chosen pill. */
  chosen?: DesignOptionLabel;
  /** A pick started the plan rewrite: the footer shows `plan_running`. */
  planRunning?: boolean;
  /** "Choose X and update plan" under the cursor at the moment of the click. */
  choosePressed?: boolean;
}> = ({ active, chosen, planRunning = false, choosePressed = false }) => {
  // footerFor(): plan_running → text; the option work follows (the chosen one, or the
  // plan run's recommendation before any pick) → "Work follows this option."; else the button.
  const selected = chosen ?? DARK_MODE_DESIGNS.recommended;
  const footerText = planRunning ? DESIGN.planRunning : active === selected ? DESIGN.workFollows : null;
  const showButton = !planRunning && active !== selected;
  return (
    <div className="absolute inset-0 z-[60]">
      <div className="absolute inset-0 bg-overlay" />
      {/* h-[92vh] and w-[calc(100%-2rem)] of the 1280×720 window, centred by inset-0 m-auto. */}
      <div className="absolute inset-0 m-auto flex h-[662px] w-[1248px] flex-col gap-3 overflow-hidden rounded-xl border border-border bg-paper-50 p-0 shadow-lg">
        <div className="flex flex-col gap-1 border-b border-border px-5 py-4">
          <h2 className="font-display text-base font-semibold text-bark-900">{DESIGN.dialogTitle}</h2>
          <p className="font-body text-sm text-text-secondary">{DARK_MODE_DESIGNS.setTitle}</p>
        </div>

        <div className="flex flex-wrap gap-2 px-5">
          {DARK_MODE_DESIGNS.options.map((option) => {
            const isActive = option.label === active;
            return (
              <div
                key={option.label}
                className={`inline-flex items-center gap-1.5 rounded-md border px-3 py-1.5 font-body text-sm ${
                  isActive ? "border-accent text-text-primary" : "border-border text-text-secondary"
                }`}
              >
                {DESIGN.optionName(option)}
                {DARK_MODE_DESIGNS.recommended === option.label && (
                  <span className="rounded-full bg-accent px-1.5 text-xs font-medium text-white">
                    {DESIGN.recommended}
                  </span>
                )}
                {chosen === option.label && (
                  <span className="rounded-full border border-border px-1.5 text-xs text-text-secondary">
                    {DESIGN.chosen}
                  </span>
                )}
              </div>
            );
          })}
        </div>

        <div className="min-h-0 flex-1 px-5">
          <Preview label={active} />
        </div>

        <div className="flex flex-wrap items-center justify-between gap-3 border-t border-border px-5 py-3">
          <div className="font-body text-sm text-text-secondary">{footerText}</div>
          {showButton && <Button pressed={choosePressed}>{DESIGN.chooseAndUpdate(active)}</Button>}
        </div>

        <div className="absolute top-4 right-4 rounded-md p-1 text-text-secondary">
          <X className="size-4" aria-hidden="true" />
        </div>
      </div>
    </div>
  );
};
