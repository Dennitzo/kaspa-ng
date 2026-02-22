type ExplorerRuntimeConfig = {
  apiBase?: string;
  socketUrl?: string;
  socketPath?: string;
  networkId?: string;
};

const globalConfig = (globalThis as { __KASPA_EXPLORER_CONFIG__?: ExplorerRuntimeConfig })
  .__KASPA_EXPLORER_CONFIG__ ?? {};

const normalizeBase = (value: string) => value.replace(/\/+$/, "");

export const API_BASE = normalizeBase(globalConfig.apiBase ?? "https://api.kaspa.org");
export const SOCKET_URL = globalConfig.socketUrl ?? "wss://t2-3.kaspa.ws";
export const SOCKET_PATH = globalConfig.socketPath ?? "/ws/socket.io";
export const NETWORK_ID = globalConfig.networkId ?? "mainnet";
