type ExplorerRuntimeConfig = {
  apiBase?: string;
  socketUrl?: string;
  socketPath?: string;
  networkId?: string;
  apiSource?: ExplorerApiSource;
};

type ExplorerApiSource = "official" | "self-hosted" | "custom";

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
const normalizeSocketUrl = (value: string) => {
  const normalized = normalizeBase(value);
  if (normalized.startsWith("wss://")) return `https://${normalized.slice("wss://".length)}`;
  if (normalized.startsWith("ws://")) return `http://${normalized.slice("ws://".length)}`;
  return normalized;
};

export const getApiBase = () => normalizeBase(readRuntimeConfig().apiBase ?? "https://api.kaspa.org");
export const getSocketUrl = () => normalizeSocketUrl(readRuntimeConfig().socketUrl ?? "https://api.kaspa.org");
export const getSocketPath = () => readRuntimeConfig().socketPath ?? "/ws/socket.io";
export const getNetworkId = () => readRuntimeConfig().networkId ?? "mainnet";
export const getApiSource = (): ExplorerApiSource => {
  const runtimeSource = readRuntimeConfig().apiSource;
  if (runtimeSource) return runtimeSource;

  const apiBase = getApiBase();
  try {
    const url = new URL(apiBase);
    const host = url.hostname.toLowerCase();
    if (host === "127.0.0.1" || host === "localhost" || host === "::1") {
      return "self-hosted";
    }
  } catch {
    // Fall through to default.
  }

  return apiBase === "https://api.kaspa.org" ? "official" : "custom";
};

export const getApiSourceLabel = () => {
  const source = getApiSource();
  if (source === "self-hosted") return "Self-hosted";
  if (source === "official") return "Public";
  return "Custom";
};

export const getApiDisplay = () => {
  const apiBase = getApiBase();
  try {
    const url = new URL(apiBase);
    return `${url.host}${url.pathname === "/" ? "" : url.pathname}`;
  } catch {
    return apiBase;
  }
};

// Backward compatibility for modules that still import constants.
export const API_BASE = getApiBase();
export const SOCKET_URL = getSocketUrl();
export const SOCKET_PATH = getSocketPath();
export const NETWORK_ID = getNetworkId();
