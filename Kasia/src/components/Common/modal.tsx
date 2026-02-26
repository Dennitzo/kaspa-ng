import { FC, ReactNode, useEffect, useState } from "react";
import { X } from "lucide-react";
import clsx from "clsx";

// Basic modal component to standardise the look
// Pass in your children - this just provides the bare minimum
export const Modal: FC<{
  onClose: () => void;
  children: ReactNode;
  className?: string;
}> = ({ onClose, children, className }) => {
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    setMounted(true);

    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        handleClose();
      }
    };

    document.addEventListener("keydown", handleEscape);
    return () => document.removeEventListener("keydown", handleEscape);
  }, []);

  const handleClose = () => {
    setMounted(false);
    setTimeout(() => {
      onClose();
    }, 200);
  };

  return (
    <div
      className={clsx(
        "fixed inset-0 z-50 flex items-center justify-center bg-black/50 transition-opacity duration-200",
        {
          "opacity-0": !mounted,
          "opacity-100": mounted,
        }
      )}
      onClick={handleClose}
    >
      <div
        className={clsx(
          "border-primary-border bg-secondary-bg relative mx-2 max-h-[95vh] w-full max-w-2xl overflow-x-hidden overflow-y-auto rounded-2xl border p-3 shadow-2xl shadow-(color:--button-primary)/30 transition-all duration-200 sm:mx-4 sm:p-6",
          {
            "translate-y-0 opacity-100": mounted,
            "-translate-y-5 opacity-0": !mounted,
          },
          className
        )}
        onClick={(e) => e.stopPropagation()}
      >
        <button
          onClick={handleClose}
          className="hover:text-kas-secondary absolute top-0.5 right-0.5 z-60 cursor-pointer p-1 hover:scale-110 active:scale-90 active:opacity-80 sm:top-2 sm:right-2 sm:p-0"
        >
          <X className="h-7 w-7 rounded-3xl" />
        </button>
        {children}
      </div>
    </div>
  );
};
