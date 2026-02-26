import { FC } from "react";
import { Radio } from "lucide-react";
import clsx from "clsx";
import { BroadcastChannel } from "../../../store/repository/broadcast-channel.repository";
import { HoldToDelete } from "../../Common/HoldToDelete";

export const BroadcastCard: FC<{
  channel: BroadcastChannel;
  onDelete?: (channelName: string) => void;
  onClick?: (channel: BroadcastChannel) => void;
  isSelected?: boolean;
  collapsed?: boolean;
}> = ({
  channel,
  onDelete,
  onClick,
  isSelected = false,
  collapsed = false,
}) => {
  if (collapsed) {
    return (
      <div
        className="relative flex cursor-pointer justify-center py-2"
        title={channel.channelName}
        onClick={() => onClick?.(channel)}
      >
        <div className="relative h-8 w-8">
          <div
            className={clsx(
              "flex h-8 w-8 items-center justify-center rounded-full transition-colors",
              "bg-[var(--kas-primary)]/20 text-[var(--kas-primary)]",
              { "ring-2 ring-[var(--kas-primary)]": isSelected }
            )}
          >
            <Radio className="h-4 w-4" />
          </div>
        </div>
      </div>
    );
  }

  return (
    <div
      className={clsx(
        "group border-primary-border relative cursor-pointer border-b p-4 transition-all duration-200",
        {
          "bg-primary-bg": isSelected,
          "hover:bg-primary-bg/50": !isSelected,
        }
      )}
      onClick={() => onClick?.(channel)}
    >
      <div className="flex items-center gap-3">
        {/* Icon */}
        <div className="relative flex-shrink-0">
          <div
            className={clsx(
              "flex h-10 w-10 items-center justify-center rounded-full transition-colors",
              "bg-[var(--kas-primary)]/20 text-[var(--kas-primary)]",
              { "animate-pulse ring-2 ring-[var(--kas-primary)]": isSelected }
            )}
          >
            <Radio className="h-5 w-5" />
          </div>
        </div>

        {/* Channel Info */}
        <div className="min-w-0 flex-1">
          <div className="mb-1 text-base font-semibold">
            <span className="block w-full cursor-pointer truncate text-[var(--text-primary)]">
              {channel.channelName}
            </span>
          </div>
          <div
            className={clsx(
              "overflow-hidden text-sm whitespace-nowrap text-[var(--text-secondary)] transition-all duration-200",
              onDelete && "group-hover:text-ellipsis"
            )}
          >
            {channel.channelValue}
          </div>
          <div className="mt-1 text-xs text-[var(--text-secondary)]">
            {channel.timestamp.toLocaleString()}
          </div>
        </div>

        {/* Delete Button */}
        {onDelete && (
          <HoldToDelete
            onComplete={() => onDelete(channel.channelName)}
            size="md"
            className="absolute top-1/2 right-2 z-10 -translate-y-1/2 opacity-100 transition-all duration-600 sm:opacity-0 sm:group-hover:opacity-70"
            title="Click and hold to delete channel"
            hoverClass="hover:text-red-500"
          />
        )}
      </div>
    </div>
  );
};
