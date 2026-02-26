import { FC, ReactNode, useState, useRef } from "react";
import clsx from "clsx";

interface HoldToActionProps {
  children: ReactNode;
  holdDuration?: number;
  animationDelay?: number;
  onShortPress?: () => void;
  onLongPress?: () => void;
  onAnimationStart?: () => void;
  onClick?: () => void;
  size?: "sm" | "md" | "lg" | "xl";
  className?: string;
  ariaLabel?: string;
  disabled?: boolean;
  ringColor?: string;
  hoverClass?: string;
}

// This is a generic component that you can pass a child icon to and
// call long press or shortpress actions from
export const HoldToAction: FC<HoldToActionProps> = ({
  children,
  holdDuration = 750,
  animationDelay = 0,
  onShortPress,
  onLongPress,
  onAnimationStart,
  onClick,
  size = "md",
  className = "",
  ariaLabel,
  disabled = false,
  ringColor = "var(--kas-primary)",
  hoverClass,
}) => {
  const [isHolding, setIsHolding] = useState(false);
  const [holdProgress, setHoldProgress] = useState(0);

  const rafIdRef = useRef<number | ReturnType<typeof setTimeout> | null>(null);
  const startTimeRef = useRef<number | null>(null);
  const completedRef = useRef(false);
  const animationStartedRef = useRef(false);
  const isPressedRef = useRef(false);

  const sizeClasses = {
    sm: "size-8",
    md: "size-10",
    lg: "size-12",
    xl: "size-14",
  };

  const stopAnimation = () => {
    if (rafIdRef.current !== null) {
      if (typeof rafIdRef.current === "number") {
        cancelAnimationFrame(rafIdRef.current);
      } else {
        clearTimeout(rafIdRef.current);
      }
    }
    rafIdRef.current = null;
    startTimeRef.current = null;
  };

  const startHold = () => {
    // cancel any existing timer/animation
    stopAnimation();

    completedRef.current = false;
    startTimeRef.current = null;
    animationStartedRef.current = false;
    isPressedRef.current = true;

    // delay starting the animation if specified
    const delayTimer = setTimeout(() => {
      // only start animation if button is still pressed
      if (!isPressedRef.current) return;

      animationStartedRef.current = true;
      setIsHolding(true);
      setHoldProgress(0);
      onAnimationStart?.();

      const tick = (ts: number) => {
        if (startTimeRef.current === null) startTimeRef.current = ts;
        const elapsed = ts - startTimeRef.current;
        const pct = Math.min(100, (elapsed / holdDuration) * 100);
        setHoldProgress(pct);
        if (pct >= 100 && !completedRef.current) {
          completedRef.current = true;
          onLongPress?.();
          setIsHolding(false);
          stopAnimation();
          setHoldProgress(0);
          return;
        }
        rafIdRef.current = requestAnimationFrame(tick);
      };
      rafIdRef.current = requestAnimationFrame(tick);
    }, animationDelay);

    rafIdRef.current = delayTimer;
  };

  const stopHold = () => {
    stopAnimation();
    setIsHolding(false);
    setHoldProgress(0);
    // don't reset completedRef if it was completed successfully
    if (!completedRef.current) {
      animationStartedRef.current = false;
    }
    isPressedRef.current = false;
  };

  const handleButtonPress = (e: React.MouseEvent | React.TouchEvent) => {
    if (disabled) return;

    e.preventDefault();
    e.stopPropagation();

    // if onClick is provided, use immediate click behavior
    if (onClick) {
      onClick();
      return;
    }

    startHold();

    const handleRelease = (releaseEvent: MouseEvent | TouchEvent) => {
      releaseEvent.preventDefault();
      releaseEvent.stopPropagation();

      // only trigger short press if animation never started and not completed
      if (!completedRef.current && !animationStartedRef.current) {
        onShortPress?.();
      }

      stopHold();

      document.removeEventListener("mouseup", handleRelease);
      document.removeEventListener("touchend", handleRelease);
    };

    document.addEventListener("mouseup", handleRelease);
    document.addEventListener("touchend", handleRelease);
  };

  // calculate circumference for outer ring (radius 18 to fill the space)
  const C = 2 * Math.PI * 18;

  // check if className contains absolute positioning
  const hasAbsolutePositioning =
    className &&
    (className.includes("absolute") ||
      className.includes("fixed") ||
      className.includes("relative"));

  return (
    <div
      className={clsx(
        hasAbsolutePositioning
          ? ""
          : "relative flex items-center justify-center",
        sizeClasses[size],
        className
      )}
    >
      <button
        aria-label={ariaLabel}
        disabled={disabled}
        className={clsx(
          "flex cursor-pointer items-center justify-center text-[var(--text-primary)] select-none focus:outline-none",
          hoverClass,
          hasAbsolutePositioning ? "" : "relative",
          sizeClasses[size]
        )}
        onMouseDown={handleButtonPress}
        onTouchStart={handleButtonPress}
        onMouseUp={stopHold}
        onMouseLeave={stopHold}
        onTouchEnd={stopHold}
        onBlur={stopHold}
        onClick={(e) => e.preventDefault()}
      >
        {/* span to increate butotn push area on touch devices */}
        <span className="absolute w-full p-7 pointer-fine:hidden"></span>
        {isHolding && (
          <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
            <svg
              className="h-full w-full -rotate-90 transform"
              viewBox="0 0 40 40"
            >
              <circle
                cx="20"
                cy="20"
                r="18"
                stroke={ringColor}
                strokeWidth="3"
                fill="none"
                strokeLinecap="round"
                strokeDasharray={`${C}`}
                strokeDashoffset={`${C * (1 - holdProgress / 100)}`}
              />
            </svg>
          </div>
        )}
        {children}
      </button>
    </div>
  );
};
