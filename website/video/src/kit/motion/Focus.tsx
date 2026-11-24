import React from "react";
import { interpolate, useCurrentFrame } from "remotion";
import { VIDEO } from "../tokens";

export type FocusKeyframe = {
  frame: number;
  x: number;
  y: number;
  scale: number;
};

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/**
 * Zooms the canvas so the keyframe point (x, y) moves towards the frame centre.
 * The view is clamped so it never shows outside the canvas; at scale 1 it is identity.
 */
export const Focus: React.FC<{
  keyframes: FocusKeyframe[];
  children: React.ReactNode;
}> = ({ keyframes, children }) => {
  const frame = useCurrentFrame();
  const frames = keyframes.map((k) => k.frame);
  const opts = {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  } as const;
  const one = keyframes.length === 1;
  const at = (pick: (k: FocusKeyframe) => number) =>
    one
      ? pick(keyframes[0])
      : interpolate(frame, frames, keyframes.map(pick), opts);

  const s = at((k) => k.scale);
  const px = at((k) => k.x);
  const py = at((k) => k.y);
  const { width: W, height: H } = VIDEO;
  const fx = clamp(px, W / (2 * s), W - W / (2 * s));
  const fy = clamp(py, H / (2 * s), H - H / (2 * s));

  return (
    <div
      className="absolute inset-0"
      style={{
        transformOrigin: "0 0",
        translate: `${W / 2 - s * fx}px ${H / 2 - s * fy}px`,
        scale: s,
      }}
    >
      {children}
    </div>
  );
};
