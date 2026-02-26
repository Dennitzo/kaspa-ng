import { Popover, PopoverButton, PopoverPanel } from "@headlessui/react";
import { FC } from "react";

export const AppVersion: FC = () => {
  return (
    <Popover className="relative">
      <PopoverButton className="text-text-primary/60 cursor-pointer">
        v{__APP_VERSION__}
      </PopoverButton>
      <PopoverPanel
        anchor="top"
        transition
        className="text-text-primary/90 border-kas-primary/80 z-[60] flex flex-col gap-1 rounded border-1 bg-[var(--primary-bg)] p-4 shadow-2xl ring-1 shadow-(color:--button-primary)/30 ring-[var(--primary-border)] transition duration-200 ease-out data-closed:scale-95 data-closed:opacity-0"
      >
        <div className="flex gap-2">
          <span>Version:</span>
          <span className="font-semibold">{__APP_VERSION__}</span>
        </div>
        <div className="flex gap-2">
          <span>Commit SHA:</span>
          <span className="font-semibold">{__COMMIT_SHA__}</span>
        </div>
      </PopoverPanel>
    </Popover>
  );
};
