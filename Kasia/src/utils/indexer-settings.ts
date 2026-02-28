const INDEXER_DISABLED_KEY = "kasia_indexer_disabled";
const INDEXER_URL_KEY = "kasia_indexer_url";
const INDEXER_CONNECTION_MODE_KEY = "kasia_indexer_connection_mode";
const NODE_CONNECTION_MODE_KEY = "kasia_node_connection_mode";

// gets whether the indexer is disabled.
// checks localStorage. Defaults to false (enabled) if no value is set.
export function isIndexerDisabled(): boolean {
  const localStorageValue = localStorage.getItem(INDEXER_DISABLED_KEY);
  return localStorageValue === "true";
}

// sets the indexer disabled setting in localStorage.
export function setIndexerDisabled(disabled: boolean): void {
  localStorage.setItem(INDEXER_DISABLED_KEY, disabled.toString());
}

// gets the indexer URL from localStorage.
export function getIndexerUrl(): string | null {
  return localStorage.getItem(INDEXER_URL_KEY);
}

// Sets the indexer URL in localStorage.
export function setIndexerUrl(url: string | null): void {
  if (url) {
    localStorage.setItem(INDEXER_URL_KEY, url);
  } else {
    localStorage.removeItem(INDEXER_URL_KEY);
  }
}

// gets the indexer connection mode from localStorage.
// defaults to "auto" if no value is set.
export function getIndexerConnectionMode(): "auto" | "manual" {
  const localStorageValue = localStorage.getItem(INDEXER_CONNECTION_MODE_KEY);
  return localStorageValue === "manual" ? "manual" : "auto";
}

// sets the indexer connection mode in localStorage.
export function setIndexerConnectionMode(mode: "auto" | "manual"): void {
  localStorage.setItem(INDEXER_CONNECTION_MODE_KEY, mode);
}

// gets the node connection mode from localStorage.
// defaults to "auto" if no value is set.
export function getNodeConnectionMode(): "auto" | "manual" {
  const localStorageValue = localStorage.getItem(NODE_CONNECTION_MODE_KEY);
  return localStorageValue === "manual" ? "manual" : "auto";
}

// sets the node connection mode in localStorage.
export function setNodeConnectionMode(mode: "auto" | "manual"): void {
  localStorage.setItem(NODE_CONNECTION_MODE_KEY, mode);
}

export function getEffectiveIndexerUrl(network: "mainnet" | "testnet"): string {
  const runtimeConfig =
    (globalThis as {
      __KASPA_NG_KASIA_CONFIG?: {
        indexerMainnetUrl?: string;
        indexerTestnetUrl?: string;
      };
    }).__KASPA_NG_KASIA_CONFIG ?? {};
  const connectionMode = getIndexerConnectionMode();
  const customUrl = getIndexerUrl();

  if (connectionMode === "manual" && customUrl) {
    return customUrl;
  }

  if (connectionMode === "manual" && !customUrl) {
    setIndexerConnectionMode("auto");
  }

  return network === "mainnet"
    ? runtimeConfig.indexerMainnetUrl ?? import.meta.env.VITE_INDEXER_MAINNET_URL
    : runtimeConfig.indexerTestnetUrl ?? import.meta.env.VITE_INDEXER_TESTNET_URL;
}

export function getEffectiveNodeUrl(
  network: "mainnet" | "testnet",
  preferredUrl?: string | null
): string {
  const normalizedPreferred =
    typeof preferredUrl === "string" ? preferredUrl.trim() : "";
  if (normalizedPreferred.length > 0) {
    return normalizedPreferred;
  }

  const runtimeConfig =
    (globalThis as {
      __KASPA_NG_KASIA_CONFIG?: {
        defaultMainnetNodeUrl?: string;
        defaultTestnetNodeUrl?: string;
      };
    }).__KASPA_NG_KASIA_CONFIG ?? {};

  return network === "mainnet"
    ? runtimeConfig.defaultMainnetNodeUrl ??
        import.meta.env.VITE_DEFAULT_MAINNET_KASPA_NODE_URL
    : runtimeConfig.defaultTestnetNodeUrl ??
        import.meta.env.VITE_DEFAULT_TESTNET_KASPA_NODE_URL;
}

export function toAddressPortDisplay(url: string | null): string {
  if (!url) {
    return "n/a";
  }

  try {
    const parsed = new URL(url);
    if (parsed.port) {
      return `${parsed.hostname}:${parsed.port}`;
    }
    return parsed.hostname;
  } catch {
    return url
      .replace(/^https?:\/\//, "")
      .replace(/^wss?:\/\//, "")
      .replace(/\/+$/, "");
  }
}

function isPrivateIpv4Host(hostname: string): boolean {
  if (/^10\./.test(hostname)) return true;
  if (/^192\.168\./.test(hostname)) return true;
  const match172 = hostname.match(/^172\.(\d{1,3})\./);
  if (match172) {
    const octet = Number(match172[1]);
    if (octet >= 16 && octet <= 31) return true;
  }
  return false;
}

export function isSelfHostedUrl(url: string): boolean {
  try {
    const parsed = new URL(url);
    const host = parsed.hostname.toLowerCase();
    return (
      host === "localhost" ||
      host === "127.0.0.1" ||
      host === "::1" ||
      host === "0.0.0.0" ||
      isPrivateIpv4Host(host)
    );
  } catch {
    const normalized = toAddressPortDisplay(url).toLowerCase();
    return normalized.startsWith("127.0.0.1") || normalized.startsWith("localhost");
  }
}

export function getNodeStatusMeta(
  network: "mainnet" | "testnet",
  preferredUrl?: string | null
): {
  kind: "self-hosted" | "official";
  addressPort: string;
} {
  const effective = getEffectiveNodeUrl(network, preferredUrl);
  return {
    kind: isSelfHostedUrl(effective) ? "self-hosted" : "official",
    addressPort: toAddressPortDisplay(effective),
  };
}

export function getIndexerStatusLabel(network: "mainnet" | "testnet"): string {
  if (isIndexerDisabled()) {
    return "Indexer Off";
  }

  const effective = getEffectiveIndexerUrl(network);
  const kind = isSelfHostedUrl(effective) ? "Self-hosted" : "Official";
  return `Indexer ${kind} ${toAddressPortDisplay(effective)}`;
}

export function getIndexerStatusMeta(network: "mainnet" | "testnet"): {
  kind: "off" | "self-hosted" | "official";
  addressPort: string;
} {
  if (isIndexerDisabled()) {
    return { kind: "off", addressPort: "n/a" };
  }

  const effective = getEffectiveIndexerUrl(network);
  return {
    kind: isSelfHostedUrl(effective) ? "self-hosted" : "official",
    addressPort: toAddressPortDisplay(effective),
  };
}
