import React from "react";
import { interpolate, Sequence, useCurrentFrame } from "remotion";
import { assertCaption } from "./caption-limit.ts";

const FADE = 8;

const CaptionBar: React.FC<{ text: string; durationInFrames: number }> = ({
  text,
  durationInFrames,
}) => {
  const frame = useCurrentFrame();
  const opacity = interpolate(
    frame,
    [0, FADE, durationInFrames - FADE, durationInFrames],
    [0, 1, 1, 0],
    { extrapolateLeft: "clamp", extrapolateRight: "clamp" },
  );
  return (
    <div
      className="absolute inset-x-0 bottom-[48px] flex justify-center"
      style={{ opacity }}
    >
      <div className="bg-overlay-strong text-text-primary font-display rounded-xl px-[28px] py-[12px] text-[36px] leading-tight font-medium whitespace-nowrap">
        {text}
      </div>
    </div>
  );
};

export const Caption: React.FC<{
  text: string;
  from: number;
  durationInFrames: number;
}> = ({ text, from, durationInFrames }) => {
  assertCaption(text);
  return (
    <Sequence from={from} durationInFrames={durationInFrames} layout="none">
      <CaptionBar text={text} durationInFrames={durationInFrames} />
    </Sequence>
  );
};
