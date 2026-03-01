import { FC, useEffect, useMemo, useState } from "react";
import { ThemeToggle } from "../Common/ThemeToggle";
import { useNavigate } from "react-router";
import { DatabaseZap } from "lucide-react";
import {
  getEffectiveIndexerUrl,
  getIndexerStatusMeta,
  getNodeStatusMeta,
  isIndexerDisabled,
} from "../../utils/indexer-settings";
import { ConnectionIndicator } from "../Common/ConnectionIndicator";
import { useNetworkStore } from "../../store/network.store";
import { checkIndexerHealth, type IndexerStatus } from "../../utils/indexer-validation";

type Props = {
  isWalletReady: boolean;
  walletAddress?: string;
  onCloseWallet: () => void;
};

export const Header: FC<Props> = () => {
  const navigate = useNavigate();
  const selectedNetwork = useNetworkStore((state) => state.network);
  const network = selectedNetwork === "mainnet" ? "mainnet" : "testnet";
  const nodeUrl = useNetworkStore((state) => state.nodeUrl);
  const rpcUrl = useNetworkStore((state) => state.rpc.url ?? null);
  const isNodeConnected = useNetworkStore((state) => state.isConnected);
  const isNodeConnecting = useNetworkStore((state) => state.isConnecting);
  const nodePreferredUrl = useMemo(() => {
    const connected = typeof rpcUrl === "string" ? rpcUrl.trim() : "";
    if (connected.length > 0) {
      return connected;
    }

    const configured = typeof nodeUrl === "string" ? nodeUrl.trim() : "";
    return configured.length > 0 ? configured : null;
  }, [nodeUrl, rpcUrl]);
  const nodeStatus = useMemo(
    () => getNodeStatusMeta(network, nodePreferredUrl),
    [network, nodePreferredUrl]
  );
  const indexerStatus = getIndexerStatusMeta(network);
  const effectiveIndexerUrl = useMemo(
    () => getEffectiveIndexerUrl(network),
    [network]
  );
  const [health, setHealth] = useState<"checking" | IndexerStatus>("checking");

  useEffect(() => {
    if (indexerStatus.kind === "off") {
      setHealth("error");
      return;
    }

    let cancelled = false;
    let intervalId: ReturnType<typeof setInterval> | null = null;

    const probe = async () => {
      if (!cancelled) {
        setHealth("checking");
      }
      const status = await checkIndexerHealth(effectiveIndexerUrl);
      if (!cancelled) {
        setHealth(status);
      }
    };

    void probe();
    intervalId = setInterval(() => {
      void probe();
    }, 12000);

    return () => {
      cancelled = true;
      if (intervalId) clearInterval(intervalId);
    };
  }, [effectiveIndexerUrl, indexerStatus.kind]);

  const nodeTone =
    nodeStatus.kind === "self-hosted"
      ? {
          chip: "text-[var(--text-primary)]",
          label: "Self-hosted",
        }
      : {
          chip: "text-[var(--text-primary)]",
          label: "Public",
        };
  const nodeHealthTone = isNodeConnected
    ? {
        dot: "bg-[var(--accent-green)]",
        border: "border-[var(--accent-green)]/40",
        text: "Connected",
      }
    : isNodeConnecting
      ? {
          dot: "bg-[var(--accent-yellow)] animate-pulse",
          border: "border-[var(--accent-yellow)]/30",
          text: "Connecting",
        }
      : {
          dot: "bg-[var(--accent-red)]",
          border: "border-[var(--accent-red)]/40",
          text: "Disconnected",
        };

  const tone =
    indexerStatus.kind === "self-hosted"
      ? {
          chip: "text-[var(--text-primary)]",
          label: "Self-hosted",
        }
      : indexerStatus.kind === "official"
        ? {
            chip: "text-[var(--text-primary)]",
            label: "Public",
          }
        : {
            chip: "text-[var(--text-primary)]",
            label: "Off",
          };
  const healthTone =
    health === "success"
      ? {
          dot: "bg-[var(--accent-green)]",
          border: "border-[var(--accent-green)]/40",
          text: "Connected",
        }
      : health === "reachable"
        ? {
            dot: "bg-[var(--accent-yellow)]",
            border: "border-[var(--accent-yellow)]/40",
            text: "Reachable",
          }
        : health === "checking"
          ? {
              dot: "bg-[var(--accent-yellow)] animate-pulse",
              border: "border-[var(--accent-yellow)]/30",
              text: "Checking",
            }
          : {
              dot: "bg-[var(--accent-red)]",
              border: "border-[var(--accent-red)]/40",
              text: "Unreachable",
            };

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
        <div
          className={`bg-primary-bg/60 hidden items-center gap-2 rounded-full border px-3 py-1 text-xs sm:flex ${nodeTone.chip} ${nodeHealthTone.border}`}
        >
          <span className={`h-2 w-2 rounded-full ${nodeHealthTone.dot}`} />
          <span className="font-medium">Node {nodeTone.label}</span>
          <span className="text-[var(--text-secondary)]">
            {nodeStatus.addressPort}
          </span>
          <span className="text-[var(--text-secondary)]/90">
            {nodeHealthTone.text}
          </span>
        </div>
        <div
          className={`bg-primary-bg/60 hidden items-center gap-2 rounded-full border px-3 py-1 text-xs sm:flex ${tone.chip} ${healthTone.border}`}
        >
          <span className={`h-2 w-2 rounded-full ${healthTone.dot}`} />
          <span className="font-medium">Indexer {tone.label}</span>
          <span className="text-[var(--text-secondary)]">
            {indexerStatus.addressPort}
          </span>
          <span className="text-[var(--text-secondary)]/90">
            {healthTone.text}
          </span>
        </div>
        <ConnectionIndicator />
        {isIndexerDisabled() && (
          <DatabaseZap className="size-5 text-[var(--accent-red)]/80" />
        )}
        <ThemeToggle />
      </div>
    </div>
  );
};
