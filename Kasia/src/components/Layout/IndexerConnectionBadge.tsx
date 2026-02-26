import { FC, useEffect, useMemo, useState } from "react";
import { useNetworkStore } from "../../store/network.store";
import { getEffectiveIndexerUrl } from "../../utils/indexer-settings";

type ProbeState = "unknown" | "checking" | "online" | "offline";

const LOCAL_HOSTS = new Set(["127.0.0.1", "localhost", "::1"]);

const trimTrailingSlash = (value: string) => value.replace(/\/+$/, "");

const parseUrl = (value: string): URL | null => {
  try {
    return new URL(value);
  } catch {
    return null;
  }
};

const hostPortFromUrl = (value: string): string | null => {
  const parsed = parseUrl(value);
  if (!parsed) return null;
  return parsed.port ? `${parsed.hostname}:${parsed.port}` : parsed.hostname;
};

const getEmbeddedRuntimeIndexerUrl = (
  network: "mainnet" | "testnet"
): string => {
  const runtimeConfig =
    (globalThis as {
      __KASPA_NG_KASIA_CONFIG?: {
        indexerMainnetUrl?: string;
        indexerTestnetUrl?: string;
      };
    }).__KASPA_NG_KASIA_CONFIG ?? {};

  return network === "mainnet"
    ? runtimeConfig.indexerMainnetUrl ?? ""
    : runtimeConfig.indexerTestnetUrl ?? "";
};

const sourceFromUrl = (value: string): "local" | "official" | "custom" => {
  const parsed = parseUrl(value);
  if (!parsed) return "custom";
  if (LOCAL_HOSTS.has(parsed.hostname)) return "local";
  if (
    parsed.hostname === "indexer.kasia.fyi" ||
    parsed.hostname === "dev-indexer.kasia.fyi"
  ) {
    return "official";
  }
  return "custom";
};

export const IndexerConnectionBadge: FC = () => {
  const network = useNetworkStore((s) => s.network);
  const [probeState, setProbeState] = useState<ProbeState>("unknown");
  const net = network === "mainnet" ? "mainnet" : "testnet";

  const baseUrl = useMemo(() => getEffectiveIndexerUrl(net), [net]);
  const embeddedUrl = useMemo(() => getEmbeddedRuntimeIndexerUrl(net), [net]);
  const displayUrl = useMemo(() => embeddedUrl || baseUrl, [embeddedUrl, baseUrl]);

  const source = useMemo(
    () => (embeddedUrl ? "custom" : sourceFromUrl(displayUrl)),
    [displayUrl, embeddedUrl]
  );
  const endpointHostPort = useMemo(
    () => hostPortFromUrl(displayUrl),
    [displayUrl]
  );

  useEffect(() => {
    let active = true;
    let timer: ReturnType<typeof setInterval> | undefined;

    if (!displayUrl) {
      setProbeState("unknown");
      return () => {
        active = false;
        if (timer) clearInterval(timer);
      };
    }

    const metricsUrl = `${trimTrailingSlash(displayUrl)}/metrics`;
    const probe = async () => {
      if (!active) return;
      setProbeState("checking");
      try {
        await fetch(metricsUrl, {
          method: "GET",
          mode: "no-cors",
          cache: "no-store",
        });
        if (active) setProbeState("online");
      } catch {
        if (active) setProbeState("offline");
      }
    };

    void probe();
    timer = setInterval(() => void probe(), 5000);

    return () => {
      active = false;
      if (timer) clearInterval(timer);
    };
  }, [displayUrl]);

  const statusText =
    probeState === "online"
      ? "online"
      : probeState === "offline"
        ? "offline"
        : probeState === "checking"
          ? "checking"
          : source === "custom" && endpointHostPort
            ? endpointHostPort
            : "n/a";

  const sourceText =
    source === "local" ? "Local" : source === "official" ? "Official" : "Custom";

  const statusColor =
    probeState === "online"
      ? "text-emerald-400"
      : probeState === "offline"
        ? "text-rose-400"
        : "text-[var(--text-secondary)]";

  return (
    <div
      className="hidden md:flex items-center gap-2 rounded-full border border-primary-border bg-[var(--primary-bg)] px-3 py-1 text-xs"
      title={`Indexer: ${displayUrl || "not configured"}`}
    >
      <span className="text-[var(--text-secondary)]">Indexer</span>
      <span className="font-semibold text-[var(--text-primary)]">{sourceText}</span>
      <span className={statusColor}>{statusText}</span>
      <span className="max-w-[260px] truncate text-[var(--text-secondary)]">
        {displayUrl || "-"}
      </span>
    </div>
  );
};
