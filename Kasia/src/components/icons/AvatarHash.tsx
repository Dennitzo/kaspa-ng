import { FC, useEffect, useRef, useState } from "react";
import { generateAddressColor } from "../MessagesPane/Utilities";
import clsx from "clsx";

const MIN_SEG = 2,
  MAX_SEG = 12,
  STATIC_SEG = 7,
  SEGMENTS = 12,
  CYCLE = 2500,
  PAUSE_RATIO = 0.1;

export const AvatarHash: FC<{
  address: string;
  size?: number;
  className?: string;
  selected?: boolean;
  isGroup?: boolean;
}> = ({ address, size = 32, className, selected = false, isGroup = false }) => {
  const [segments, setSegments] = useState(STATIC_SEG);
  const raf = useRef<number | null>(null);

  const customColor = isGroup ? generateAddressColor(address) : null;

  /* animate number of active dots when selected */
  useEffect(() => {
    if (!selected) {
      setSegments(STATIC_SEG);
      if (raf.current !== null) cancelAnimationFrame(raf.current);
      return;
    }

    const t0 = performance.now();
    const loop = (t: number) => {
      const total = CYCLE * (2 + 2 * PAUSE_RATIO);
      const phase = ((t - t0) % total) / total;

      let seg = MIN_SEG;
      if (phase < 0.5 * (1 - PAUSE_RATIO)) {
        // grow (STATIC → MAX)
        const p = phase / (0.5 * (1 - PAUSE_RATIO));
        seg = STATIC_SEG + (MAX_SEG - STATIC_SEG) * p;
      } else if (phase < 0.5) {
        // pause at MAX
        seg = MAX_SEG;
      } else if (phase < 1 - 0.5 * PAUSE_RATIO) {
        // shrink (MAX → MIN_SEG)
        const p = (phase - 0.5) / (0.5 * (1 - PAUSE_RATIO));
        seg = MAX_SEG - (MAX_SEG - MIN_SEG) * p;
      } else {
        // pause at MIN_SEG
        seg = MIN_SEG;
      }

      const next = Math.round(seg);
      if (next !== segments) setSegments(next);

      raf.current = requestAnimationFrame(loop);
    };

    raf.current = requestAnimationFrame(loop);
    return () => {
      if (raf.current !== null) cancelAnimationFrame(raf.current);
    };
  }, [selected]);

  /* deterministic 32-bit hash */
  let h = 0;
  for (let i = 0; i < address.length; i++)
    h = ((h << 5) - h + address.charCodeAt(i)) | 0;
  const hash = h >>> 0;

  const prng = (() => {
    let x = (hash ^ 0x9e3779b9) >>> 0;
    return () => {
      x ^= x << 13;
      x ^= x >>> 17;
      x ^= x << 5;
      return x >>> 0;
    };
  })();
  const order = Array.from({ length: SEGMENTS }, (_, i) => i);
  for (let i = SEGMENTS - 1; i > 0; i--) {
    const j = prng() % (i + 1);
    [order[i], order[j]] = [order[j], order[i]];
  }

  // fixed base palette, but customColor can override
  const basePalette = ["#49EACB", "#70C7BA", "#9CA3AF", "#1F2937"];
  const dynamicPalette = customColor
    ? [customColor, ...basePalette]
    : basePalette;

  const getDistributedColor = (index: number) => {
    // when static, force index 0 to be customColor
    if (selected && customColor && index === 0) return customColor;

    const paletteSize = dynamicPalette.length;
    const offset = (hash >>> 8) % paletteSize;
    return dynamicPalette[(index + offset) % paletteSize];
  };

  // rotation offset so different hashes shift the ring
  const angleOffset = (hash / 0xffffffff) * 2 * Math.PI;

  const c = size / 2,
    rDotBase = size * 0.09,
    rRing = c - rDotBase,
    start = -Math.PI / 2;

  /* precalculate dot positions */
  const base = Array.from({ length: SEGMENTS }, (_, i) => {
    const θ = start + angleOffset + (2 * Math.PI * i) / SEGMENTS;
    return { cx: c + rRing * Math.cos(θ), cy: c + rRing * Math.sin(θ) };
  });

  return (
    <svg
      width={size}
      height={size}
      viewBox={`0 0 ${size} ${size}`}
      className={clsx(
        "rounded-full bg-[var(--secondary-bg)]",
        !selected && "opacity-80",
        className
      )}
    >
      {base.map(({ cx, cy }, i) => {
        const idx = order[i];
        const isStatic = !selected;
        const forcedOn = isStatic && !!customColor && i === 0;
        const on = forcedOn || idx < segments;
        const col = getDistributedColor(i);
        const rJitter = 0.85 + (0.3 * ((hash ^ (i * 1140071481)) % 997)) / 997;
        const rDot = rDotBase * rJitter;

        return (
          <circle
            key={i}
            cx={cx}
            cy={cy}
            r={rDot}
            fill={col}
            className={clsx(
              "transition-opacity duration-300",
              !on && "opacity-0"
            )}
          />
        );
      })}
    </svg>
  );
};
