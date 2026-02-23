type ExplorerRuntimeConfig = {
  apiBase?: string;
  socketUrl?: string;
  socketPath?: string;
  networkId?: string;
};

const readQueryConfig = (): ExplorerRuntimeConfig => {
  if (typeof window === "undefined") return {};
  const params = new URLSearchParams(window.location.search);
  const config: ExplorerRuntimeConfig = {};
  const apiBase = params.get("apiBase");
  const socketUrl = params.get("socketUrl");
  const socketPath = params.get("socketPath");
  const networkId = params.get("networkId");
  if (apiBase) config.apiBase = apiBase;
  if (socketUrl) config.socketUrl = socketUrl;
  if (socketPath) config.socketPath = socketPath;
  if (networkId) config.networkId = networkId;
  return config;
};

const QUERY_CONFIG = readQueryConfig();

const readRuntimeConfig = (): ExplorerRuntimeConfig => {
  const runtime =
    (globalThis as { __KASPA_EXPLORER_CONFIG__?: ExplorerRuntimeConfig })
      .__KASPA_EXPLORER_CONFIG__ ?? {};
  return {
    ...QUERY_CONFIG,
    ...runtime,
  };
};

const normalizeBase = (value: string) => value.replace(/\/+$/, "");

export const getApiBase = () => normalizeBase(readRuntimeConfig().apiBase ?? "https://api.kaspa.org");
export const getSocketUrl = () => readRuntimeConfig().socketUrl ?? "wss://api.kaspa.org";
export const getSocketPath = () => readRuntimeConfig().socketPath ?? "/ws/socket.io";
export const getNetworkId = () => readRuntimeConfig().networkId ?? "mainnet";

// Backward compatibility for modules that still import constants.
export const API_BASE = getApiBase();
export const SOCKET_URL = getSocketUrl();
export const SOCKET_PATH = getSocketPath();
export const NETWORK_ID = getNetworkId();
