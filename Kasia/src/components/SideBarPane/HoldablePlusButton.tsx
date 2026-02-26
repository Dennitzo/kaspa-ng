import { FC } from "react";
import { Plus } from "lucide-react";
import clsx from "clsx";
import { HoldToAction } from "../Common/HoldToAction";

interface HoldablePlusButtonProps {
  broadcastEnabled: boolean;
  isBroadcastMode: boolean;
  onNewChat: () => void;
  onOffChainHandshake: () => void;
  onNewBroadcast: () => void;
  collapsed?: boolean;
}

export const HoldablePlusButton: FC<HoldablePlusButtonProps> = ({
  broadcastEnabled,
  isBroadcastMode,
  onNewChat,
  onOffChainHandshake,
  onNewBroadcast,
  collapsed = false,
}) => {
  const handleShortPress = () => {
    onNewChat();
  };

  const handleLongPress = () => {
    onOffChainHandshake();
  };

  // Use onClick for broadcast mode to bypass hold behavior
  const handleClick = isBroadcastMode ? onNewBroadcast : undefined;

  const buttonClasses = clsx(
    "hover:bg-primary-bg/50 cursor-pointer focus:outline-none active:scale-90 active:opacity-80",
    collapsed ? "rounded p-2" : "rounded p-1"
  );

  return (
    <HoldToAction
      holdDuration={1000}
      animationDelay={200}
      onShortPress={handleShortPress}
      onLongPress={handleLongPress}
      onClick={handleClick}
      size="md"
      className={buttonClasses}
      ariaLabel={broadcastEnabled ? "new channel" : "new chat"}
      ringColor="var(--kas-primary)"
      hoverClass="hover:text-[var(--kas-primary)]"
    >
      <Plus className="h-6 w-6" />
    </HoldToAction>
  );
};
