// 01-cold-open (4 s): wordmark + subline.
import React from "react";
import { AbsoluteFill, useCurrentFrame } from "remotion";
import { fade, prog } from "../lib/anim";
import { Stage, Wordmark, useMinimal } from "../lib/ui";

export const S01ColdOpen: React.FC = () => {
  const f = useCurrentFrame();
  const minimal = useMinimal();
  return (
    <Stage fadeIn>
      <AbsoluteFill style={{ justifyContent: "center", alignItems: "center", flexDirection: "column", gap: 40 }}>
        <Wordmark at={0.5} size={140} />
        {!minimal && <div style={{ fontSize: 40, color: "var(--text-2)", opacity: fade(f, 1.4), transform: `translateY(${(1 - prog(f, 1.4, 0.5)) * 12}px)` }}>
          An economy for the agents you own.
        </div>}
      </AbsoluteFill>
    </Stage>
  );
};
