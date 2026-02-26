import { FC } from "react";
import clsx from "clsx";

const formatTime = (seconds: number | undefined) => {
  if (seconds === undefined || seconds === null) return "";
  const value = Number(seconds);
  if (isNaN(value)) return "";

  if (value < 0.001) {
    return "<0.1s";
  } else if (value < 1) {
    return `~${(value * 1000).toFixed(0)}ms`;
  } else if (value < 60) {
    return `~${value.toFixed(1)}s`;
  } else {
    return `~${Math.round(value / 60)}m`;
  }
};

function getTimeClasses(seconds: number) {
  if (seconds < 1)
    return "text-[var(--accent-green)] border-[var(--accent-green)]";
  if (seconds < 5)
    return "text-[var(--accent-blue)] border-[var(--accent-blue)]";
  if (seconds < 15)
    return "text-[var(--accent-yellow)] border-[var(--accent-yellow)]";
  if (seconds < 30)
    return "text-[var(--accent-orange)] border-[var(--accent-orange)]";
  return "text-[var(--accent-red)] border-[var(--accent-red)]";
}

interface WaitDisplayProps {
  estimatedSeconds?: number;
}

export const WaitDisplay: FC<WaitDisplayProps> = ({ estimatedSeconds }) => {
  if (!estimatedSeconds || estimatedSeconds <= 0.5) return null;

  return (
    <div
      className={clsx(
        "inline-block rounded-md border bg-[var(--secondary-bg)]/20 px-3 py-1 text-right text-xs",
        getTimeClasses(estimatedSeconds)
      )}
    >
      {formatTime(estimatedSeconds)} wait
    </div>
  );
};
