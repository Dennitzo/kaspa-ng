import React from "react";
import clsx from "clsx";
import { LucideIcon, AlertTriangle } from "lucide-react";

interface WarningBlockProps {
  title: string;
  children: React.ReactNode;
  className?: string;
  icon?: LucideIcon;
  showIcon?: boolean;
  iconClassName?: string;
}

export const WarningBlock: React.FC<WarningBlockProps> = ({
  title,
  children,
  className,
  icon: Icon = AlertTriangle,
  showIcon = true,
  iconClassName = "text-text-warning size-6",
}) => {
  return (
    <div
      className={clsx(
        "border-text-warning/50 from-text-warning/5 to-text-warning/15 rounded-2xl border bg-gradient-to-r p-2 font-semibold select-none sm:p-4",
        className
      )}
    >
      {/* Icon inline with title */}
      <div className="text-text-warning mb-2 flex items-center justify-center gap-2 text-sm font-medium sm:justify-start">
        {showIcon && Icon && <Icon className={iconClassName} />}
        {title}
      </div>

      {/* Body text always left-aligned */}
      <div className="text-text-warning/80 text-center text-xs sm:text-left">
        {children}
      </div>
    </div>
  );
};
