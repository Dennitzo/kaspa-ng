const INDEXER_DISABLED_KEY = "kasia_indexer_disabled";
const INDEXER_URL_KEY = "kasia_indexer_url";
const INDEXER_CONNECTION_MODE_KEY = "kasia_indexer_connection_mode";
const NODE_CONNECTION_MODE_KEY = "kasia_node_connection_mode";

const getRuntimeIndexerDefaults = (): {
  mainnet?: string;
  testnet?: string;
} => {
  const runtimeConfig =
    (globalThis as {
      __KASPA_NG_KASIA_CONFIG?: {
        indexerMainnetUrl?: string;
        indexerTestnetUrl?: string;
      };
    }).__KASPA_NG_KASIA_CONFIG ?? {};

  return {
    mainnet: runtimeConfig.indexerMainnetUrl,
    testnet: runtimeConfig.indexerTestnetUrl,
  };
};

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
  const connectionMode = getIndexerConnectionMode();
  const customUrl = getIndexerUrl();
  const runtimeDefaults = getRuntimeIndexerDefaults();

  if (connectionMode === "manual" && customUrl) {
    return customUrl;
  }

  if (connectionMode === "manual" && !customUrl) {
    setIndexerConnectionMode("auto");
  }

  if (network === "mainnet") {
    return (
      runtimeDefaults.mainnet ??
      import.meta.env.VITE_INDEXER_MAINNET_URL ??
      ""
    );
  }

  return (
    runtimeDefaults.testnet ?? import.meta.env.VITE_INDEXER_TESTNET_URL ?? ""
  );
}
