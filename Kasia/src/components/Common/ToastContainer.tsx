import { useToastStore } from "../../store/toast.store";
import { CircleCheck, Ban, TriangleAlert, Info, XIcon } from "lucide-react";
import { useIsMobile } from "../../hooks/useIsMobile";
import { ToastType } from "../../store/toast.store";

import clsx from "clsx";

export function ToastContainer() {
  const { toasts, remove } = useToastStore();

  const isMobile = useIsMobile();

  const getToastTransform = (offset: number) => {
    if (Math.abs(offset) > 2) return { opacity: 0 };

    const absOffset = Math.abs(offset);
    // spread them out a little futher on desktop
    const translateY = -offset * (12 * (isMobile ? 1.5 : 2));
    const scale = 1 - absOffset * 0.05;
    const opacity = Math.max(0.4, 1 - absOffset * 0.2);

    return {
      transform: `translateY(${translateY}px) scale(${scale})`,
      opacity,
    };
  };

  const getToastTypeClasses = (type: ToastType) => {
    switch (type) {
      case "success":
        return "bg-[var(--accent-green)]/80 text-white";
      case "error":
        return "bg-[var(--accent-red)]/80 text-white";
      case "warning":
        return "bg-[var(--accent-yellow)]/80 text-white";
      case "info":
        return "bg-[var(--accent-blue)]/80 text-white";
    }
  };

  const getToastIcon = (type: ToastType) => {
    switch (type) {
      case "success":
        return <CircleCheck />;
      case "error":
        return <Ban />;
      case "warning":
        return <TriangleAlert />;
      case "info":
        return <Info />;
    }
  };

  // if no toasts, return
  if (toasts.length === 0) {
    return null;
  }

  return (
    <div className="fixed top-10 right-2 z-60 select-none sm:top-16 sm:right-4">
      {toasts.map((toast, index) => {
        const offset = index;

        return (
          <div
            key={toast.id}
            onClick={() => {
              remove(toast.id);
            }}
            className={clsx(
              "animate-slide absolute right-0 flex w-[90vw] items-center gap-2 overflow-hidden rounded-xl px-4 py-3 text-sm wrap-anywhere shadow-lg transition-transform duration-300 sm:w-96",
              // toast colors
              getToastTypeClasses(toast.type),
              // since we only show 3 toasts, disable click on the rest
              Math.abs(offset) <= 2 ? "cursor-pointer" : "pointer-events-none"
            )}
            style={{
              zIndex: 100 - Math.abs(offset),
              ...getToastTransform(offset),
            }}
          >
            {/* toast icon */}
            <span className="h-6 w-6 flex-shrink-0">
              {getToastIcon(toast.type)}
            </span>

            {/* message content */}
            <span className="flex-1">{toast.message}</span>

            {/* close buttons*/}
            <button
              onClick={(e) => {
                remove(toast.id);
              }}
              className="ml-auto flex-shrink-0 cursor-pointer text-lg font-bold transition-opacity hover:opacity-70"
            >
              <XIcon className="size-5" />
            </button>
          </div>
        );
      })}
    </div>
  );
}
