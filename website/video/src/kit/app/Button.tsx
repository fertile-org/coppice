import { Loader2 } from "lucide-react";
import React from "react";
import { useCurrentFrame } from "remotion";

// Mirrors web/src/components/ui/button.tsx. `primary` is the app's `default`
// variant. `pressed` shows the app's hover colours, the state under the cursor
// at the moment of a click. Transition and focus classes are dropped. The app
// merges classes with tailwind-merge; here each slot is picked explicitly so no
// two utilities of the same kind compete.
// `disabled` is the app's showDisabled style (dashed border, no fill); `loading`
// keeps the enabled fill and adds the Loader2 spinner, turned by frame.
const BASE =
  "inline-flex items-center justify-center gap-2 rounded-md font-body font-medium";

const COLOURS = {
  primary: { rest: "bg-btn-primary text-btn-primary-fg", pressed: "bg-btn-primary-hover text-btn-primary-fg" },
  secondary: {
    rest: "border border-btn-secondary-border bg-btn-secondary text-btn-secondary-fg",
    pressed: "border border-btn-secondary-border bg-btn-secondary-hover text-text-primary",
  },
  destructive: { rest: "border border-danger text-danger", pressed: "border border-danger bg-danger-muted/40 text-danger" },
  ghost: { rest: "text-text-secondary", pressed: "text-text-primary" },
} as const;

const DISABLED = "border border-dashed border-btn-disabled-border bg-transparent text-btn-disabled-fg";

const PADDING = {
  primary: "px-4 py-2",
  secondary: "px-3 py-1.5",
  destructive: "px-3 py-1.5",
  ghost: "px-3 py-1.5",
} as const;

const Spinner: React.FC = () => {
  const frame = useCurrentFrame();
  return <Loader2 className="size-4 shrink-0" style={{ rotate: `${frame * 12}deg` }} aria-hidden="true" />;
};

export const Button: React.FC<{
  variant?: "primary" | "secondary" | "destructive" | "ghost";
  pressed?: boolean;
  disabled?: boolean;
  loading?: boolean;
  size?: "default" | "sm";
  /** Replaces the variant's padding, as the app's `className="px-3 py-1.5"` does. */
  padding?: string;
  children: React.ReactNode;
}> = ({ variant = "primary", pressed = false, disabled = false, loading = false, size = "default", padding, children }) => (
  <div
    className={[
      BASE,
      disabled && !loading ? DISABLED : COLOURS[variant][pressed ? "pressed" : "rest"],
      size === "sm" ? "px-2.5 py-1 text-xs" : `${padding ?? PADDING[variant]} text-sm`,
    ].join(" ")}
  >
    {loading && <Spinner />}
    {children}
  </div>
);
