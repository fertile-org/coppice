import React from "react";
import "./index.css";
import { Composition } from "remotion";
import { VIDEO } from "./kit/tokens";
import { Hero, HERO_DURATION_FRAMES } from "./Hero/Hero";

export const RemotionRoot: React.FC = () => {
  return (
    <Composition
      id="Hero"
      component={Hero}
      durationInFrames={HERO_DURATION_FRAMES}
      fps={VIDEO.fps}
      width={VIDEO.width}
      height={VIDEO.height}
    />
  );
};
