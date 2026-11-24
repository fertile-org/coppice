import React from "react";
import { AbsoluteFill, Img, interpolate, spring, staticFile, useCurrentFrame, useVideoConfig } from "remotion";
import "../tokens";

export const IntroCard: React.FC<{ title: string }> = ({ title }) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const pop = spring({ frame, fps, config: { damping: 200 } });
  const titleIn = spring({ frame: frame - 8, fps, config: { damping: 200 } });
  return (
    <AbsoluteFill className="bg-background items-center justify-center gap-[40px]">
      <Img
        src={staticFile("logo-dark@2x.webp")}
        className="h-[256px] w-[256px]"
        style={{
          opacity: pop,
          scale: interpolate(pop, [0, 1], [0.85, 1]),
        }}
      />
      <div
        className="text-text-primary font-display text-[84px] leading-tight font-bold"
        style={{
          opacity: titleIn,
          translate: `0px ${interpolate(titleIn, [0, 1], [24, 0])}px`,
        }}
      >
        {title}
      </div>
    </AbsoluteFill>
  );
};
