import { FC } from "react";
import { ThemeToggle } from "../Common/ThemeToggle";
import { useNavigate } from "react-router";
import { DatabaseZap } from "lucide-react";
import { isIndexerDisabled } from "../../utils/indexer-settings";
import { ConnectionIndicator } from "../Common/ConnectionIndicator";
import { IndexerConnectionBadge } from "./IndexerConnectionBadge";

type Props = {
  isWalletReady: boolean;
  walletAddress?: string;
  onCloseWallet: () => void;
};

export const Header: FC<Props> = () => {
  const navigate = useNavigate();
  return (
    <div className="border-primary-border flex items-center justify-between border-b bg-[var(--secondary-bg)] px-8 py-1 text-center select-none">
      <div
        onClick={() => navigate(`/.`)}
        className="flex cursor-pointer items-center gap-2"
      >
        <img
          src="/kasia-logo.png"
          alt="Kasia Logo"
          className="-mr-6 h-[60px] w-[60px] object-contain select-none"
        />
        <div className="ml-3 text-2xl font-semibold text-[var(--text-primary)]">
          Kasia
        </div>
      </div>

      <div className="flex items-center gap-4">
        <ConnectionIndicator />
        <IndexerConnectionBadge />
        {isIndexerDisabled() && (
          <DatabaseZap className="size-5 text-[var(--accent-red)]/80" />
        )}
        <ThemeToggle />
      </div>
    </div>
  );
};
