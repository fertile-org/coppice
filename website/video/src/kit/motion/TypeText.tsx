import React from "react";
import { useCurrentFrame, useVideoConfig } from "remotion";

export const TypeText: React.FC<{
  text: string;
  startFrame: number;
  charsPerSecond?: number;
}> = ({ text, startFrame, charsPerSecond = 30 }) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const typed = Math.max(
    0,
    Math.min(text.length, Math.floor(((frame - startFrame) / fps) * charsPerSecond)),
  );
  const typing = frame >= startFrame && typed < text.length;
  // Caret blinks every 0.5 s once typing is done; solid while typing.
  const caretOn = typing || Math.floor(frame / (fps / 2)) % 2 === 0;

  return (
    <span>
      {text.slice(0, typed)}
      <span
        className="bg-text-primary ml-[2px] inline-block h-[1em] w-[2px] align-text-bottom"
        style={{ opacity: caretOn && frame >= startFrame ? 1 : 0 }}
      />
    </span>
  );
};
