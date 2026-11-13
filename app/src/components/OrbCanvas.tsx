import { useEffect, useRef } from "react";

import { addDrawer } from "../canvas/loop";
import { drawOrb } from "../canvas/orb";
import type { Orb } from "../lib/types";

interface Props {
  orb: Orb;
  seed: number;
  size?: number;
  /** Read each frame: 0..1 swell (e.g. while the preview plays). */
  pulse?: () => number;
  /** Read each frame: 0..1 converge progress for the clone reveal. */
  assemble?: () => number;
  className?: string;
}

/** An animated dot orb. Only draws while on screen. */
export function OrbCanvas({ orb, seed, size = 150, pulse, assemble, className }: Props) {
  const ref = useRef<HTMLCanvasElement>(null);
  const props = useRef({ orb, seed, pulse, assemble });
  props.current = { orb, seed, pulse, assemble };

  useEffect(() => {
    const cv = ref.current;
    if (!cv) return;
    let visible = true;
    const io = typeof IntersectionObserver !== "undefined" ? new IntersectionObserver(([e]) => (visible = e.isIntersecting)) : null;
    io?.observe(cv);
    const remove = addDrawer((t) => {
      if (!visible) return;
      const p = props.current;
      drawOrb(cv, p.orb, t, { seed: p.seed, pulse: p.pulse?.() ?? 0, assemble: p.assemble?.() ?? 1 });
    });
    return () => {
      remove();
      io?.disconnect();
    };
  }, []);

  return <canvas ref={ref} aria-hidden="true" className={className} style={{ width: size, height: size, display: "block" }} />;
}
