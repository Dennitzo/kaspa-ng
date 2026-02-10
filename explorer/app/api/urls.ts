const DEFAULT_HOST = typeof window === "undefined" ? "umbrel.local" : window.location.hostname;

const DEFAULT_API_BASE = `http://${DEFAULT_HOST}:8091`;
const DEFAULT_SOCKET_URL = `ws://${DEFAULT_HOST}:8092`;
const DEFAULT_VPROGS_BASE = `http://${DEFAULT_HOST}:19115`;

export const API_BASE = import.meta.env.VITE_KASPA_API_BASE ?? DEFAULT_API_BASE;
export const SOCKET_URL = import.meta.env.VITE_KASPA_SOCKET_URL ?? DEFAULT_SOCKET_URL;
export const VPROGS_BASE = import.meta.env.VITE_VPROGS_BASE ?? DEFAULT_VPROGS_BASE;

export const apiUrl = (path: string) => {
  const base = API_BASE.replace(/\/$/, "");
  const suffix = path.replace(/^\//, "");
  return `${base}/${suffix}`;
};
