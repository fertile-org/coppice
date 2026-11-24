import React from "react";
import { Easing, interpolate, useCurrentFrame } from "remotion";

export type CursorPoint = {
  frame: number;
  x: number;
  y: number;
  click?: boolean;
};

const RIPPLE_FRAMES = 12;
const ease = Easing.bezier(0.45, 0, 0.25, 1);

export const Cursor: React.FC<{ path: CursorPoint[] }> = ({ path }) => {
  const frame = useCurrentFrame();
  if (path.length === 0) return null;

  // Eased movement between consecutive points; holds at the ends.
  let x = path[0].x;
  let y = path[0].y;
  for (let i = 1; i < path.length; i++) {
    const a = path[i - 1];
    const b = path[i];
    if (frame >= b.frame) {
      x = b.x;
      y = b.y;
    } else if (frame > a.frame) {
      x = interpolate(frame, [a.frame, b.frame], [a.x, b.x], { easing: ease });
      y = interpolate(frame, [a.frame, b.frame], [a.y, b.y], { easing: ease });
      break;
    } else {
      break;
    }
  }

  const ripples = path.filter(
    (p) => p.click && frame >= p.frame && frame < p.frame + RIPPLE_FRAMES,
  );
  const pressed = ripples.length > 0;

  return (
    <>
      {ripples.map((p) => {
        const t = frame - p.frame;
        const size = interpolate(t, [0, RIPPLE_FRAMES], [16, 88]);
        return (
          <div
            key={p.frame}
            className="border-accent absolute rounded-full border-[4px]"
            style={{
              left: p.x - size / 2,
              top: p.y - size / 2,
              width: size,
              height: size,
              opacity: interpolate(t, [0, RIPPLE_FRAMES], [0.9, 0]),
            }}
          />
        );
      })}
      <svg
        width={44}
        height={44}
        viewBox="0 0 24 24"
        className="absolute"
        style={{
          left: x,
          top: y,
          scale: pressed ? 0.88 : 1,
          transformOrigin: "0 0",
          filter: "drop-shadow(0 2px 4px var(--color-overlay))",
        }}
      >
        <path
          d="M2 2 L2 19 L6.5 14.8 L9.6 21.8 L12.6 20.5 L9.6 13.6 L15.8 13.6 Z"
          fill="var(--color-text-primary)"
          stroke="var(--color-background)"
          strokeWidth={1.4}
          strokeLinejoin="round"
        />
      </svg>
    </>
  );
};
