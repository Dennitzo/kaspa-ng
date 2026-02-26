import { FC } from "react";
import { Trash2 } from "lucide-react";
import { HoldToAction } from "./HoldToAction";

interface HoldToDeleteProps {
  onComplete: () => void;
  holdDuration?: number;
  size?: "sm" | "md" | "lg" | "xl";
  className?: string;
  title?: string;
  hoverClass?: string;
}

export const HoldToDelete: FC<HoldToDeleteProps> = ({
  onComplete,
  holdDuration = 750,
  size = "md",
  className = "",
  title = "Click and hold to delete",
  hoverClass = "hover:text-red-500",
}) => {
  const iconSizeMap = {
    sm: "size-4",
    md: "size-5",
    lg: "size-6",
    xl: "size-7",
  };

  return (
    <HoldToAction
      holdDuration={holdDuration}
      onLongPress={onComplete}
      size={size as "sm" | "md" | "lg" | "xl"}
      className={className}
      ariaLabel={title}
      ringColor="var(--accent-red)"
      hoverClass={hoverClass}
    >
      <Trash2 className={iconSizeMap[size] || "size-5"} />
    </HoldToAction>
  );
};
