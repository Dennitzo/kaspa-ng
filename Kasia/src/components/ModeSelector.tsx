import { FC } from "react";
import { Menu, MenuButton, MenuItems, MenuItem } from "@headlessui/react";
import { MessageCircle, Radio } from "lucide-react";
import { toast } from "../utils/toast-helper";

type ModeSelectorProps = {
  onModeChange: (isBroadcastMode: boolean) => void;
  isBroadcastMode: boolean;
  shouldShow?: boolean;
};

export const ModeSelector: FC<ModeSelectorProps> = ({
  onModeChange,
  isBroadcastMode,
  shouldShow = true,
}) => {
  // Early return if shouldShow is false
  if (!shouldShow) {
    return null;
  }

  const modeIcon = isBroadcastMode ? (
    <Radio className="size-6" />
  ) : (
    <MessageCircle className="size-6" />
  );

  const modes = [
    {
      id: "chat",
      displayableString: "DMs",
      icon: <MessageCircle className="size-5" />,
      value: false,
    },
    {
      id: "radio",
      displayableString: "Broadcast",
      icon: <Radio className="size-5" />,
      value: true,
    },
  ];

  // if fewer than 3 modes, handle direct toggle with toast
  if (modes.length < 3) {
    const handleToggle = () => {
      const newMode = !isBroadcastMode;
      const selectedMode = modes.find((mode) => mode.value === newMode);

      onModeChange(newMode);
      toast.info(`Switched to ${selectedMode?.displayableString}`, 1200);
    };

    return (
      <button
        onClick={handleToggle}
        className="text-text-primary hover:bg-primary-bg/50 inline-flex size-10 cursor-pointer items-center justify-center rounded transition-colors duration-200 hover:text-[var(--kas-primary)] active:scale-90 active:opacity-80"
      >
        {modeIcon}
      </button>
    );
  }

  // behavior for 3+ modes
  // unused atm but will be used when (if) games is introduced
  return (
    <Menu>
      <MenuButton className="text-text-primary hover:bg-primary-bg/50 inline-flex size-10 cursor-pointer items-center justify-center rounded transition-colors duration-200 hover:text-[var(--kas-primary)] active:scale-90 active:opacity-80">
        {modeIcon}
      </MenuButton>
      <MenuItems
        className="absolute top-full left-0 z-10 min-w-[140px] rounded-sm border border-[var(--border-color)] bg-[var(--secondary-bg)] shadow-md"
        anchor="bottom end"
      >
        {modes.map((mode) => (
          <MenuItem key={mode.id}>
            <div
              onClick={() => onModeChange(mode.value)}
              className={`flex w-full cursor-pointer items-center gap-2 border-none bg-none px-3 py-2 text-left text-sm font-semibold text-[var(--text-primary)] transition-colors duration-200 ${
                isBroadcastMode === mode.value
                  ? "bg-[var(--kas-primary)] text-[var(--text-primary)]"
                  : "hover:bg-[var(--kas-secondary)]"
              }`}
            >
              {mode.icon}
              {mode.displayableString}
            </div>
          </MenuItem>
        ))}
      </MenuItems>
    </Menu>
  );
};
