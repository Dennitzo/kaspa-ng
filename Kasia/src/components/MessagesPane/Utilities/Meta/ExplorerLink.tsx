import type { FC } from "react";
import { Tickets } from "lucide-react";
import { getExplorerUrl } from "../../../../utils/explorer-url";
import { core } from "@tauri-apps/api";
import { openUrl } from "@tauri-apps/plugin-opener";

type ExplorerLinkProps = {
  transactionId: string;
  network: string;
  position: "left" | "right";
};

export const ExplorerLink: FC<ExplorerLinkProps> = ({
  transactionId,
  network,
  position,
}) => {
  const containerClass = position === "left" ? "ml-2" : "mr-2";

  const href = getExplorerUrl(transactionId, network);

  const handleClick: React.MouseEventHandler<HTMLAnchorElement> = async (e) => {
    if (core.isTauri()) {
      if (e.defaultPrevented) return;
      if (e.button !== 0) return;
      if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;

      e.preventDefault();
      await openUrl(href);
    }
  };

  return (
    <div className={`${containerClass} flex items-center`}>
      <a
        href={href}
        onClick={handleClick}
        target="_blank"
        rel="noopener noreferrer"
        className="text-xs opacity-80 transition-opacity hover:text-[var(--kas-primary)] active:scale-90 active:opacity-60"
      >
        <Tickets className="size-5" />
      </a>
    </div>
  );
};
