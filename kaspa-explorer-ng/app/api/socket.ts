import { useEffect, useState } from "react";
import { io } from "socket.io-client";
import { SOCKET_PATH, SOCKET_URL } from "./config";

type ExplorerRuntimeConfig = {
  socketUrl?: string;
  socketPath?: string;
};

const readRuntimeSocketConfig = () => {
  const runtime = ((globalThis as { __KASPA_EXPLORER_CONFIG__?: ExplorerRuntimeConfig })
    .__KASPA_EXPLORER_CONFIG__ ?? {}) as ExplorerRuntimeConfig;
  const socketUrl = runtime.socketUrl ?? SOCKET_URL;
  const socketPath = runtime.socketPath ?? SOCKET_PATH;
  return { socketUrl, socketPath };
};

let currentSocketConfig = readRuntimeSocketConfig();
let socketGeneration = 0;

export let socket = io(currentSocketConfig.socketUrl, {
  path: currentSocketConfig.socketPath,
  autoConnect: true,
});

export const getSocketGeneration = () => socketGeneration;

export const ensureSocketConfig = () => {
  const nextConfig = readRuntimeSocketConfig();
  if (
    nextConfig.socketUrl === currentSocketConfig.socketUrl &&
    nextConfig.socketPath === currentSocketConfig.socketPath
  ) {
    return false;
  }

  try {
    socket.removeAllListeners();
    socket.disconnect();
  } catch {
    // Ignore socket cleanup errors during hot switch.
  }

  currentSocketConfig = nextConfig;
  socket = io(currentSocketConfig.socketUrl, {
    path: currentSocketConfig.socketPath,
    autoConnect: true,
  });
  socketGeneration += 1;
  return true;
};

export const useSocketConnected = () => {
  const [connected, setConnected] = useState(false);
  const [generation, setGeneration] = useState(getSocketGeneration());

  useEffect(() => {
    const intervalId = setInterval(() => {
      if (ensureSocketConfig()) {
        setGeneration(getSocketGeneration());
        setConnected(false);
      }
    }, 1000);
    return () => clearInterval(intervalId);
  }, []);

  useEffect(() => {
    const activeSocket = socket;
    let timeoutId: NodeJS.Timeout | null = null;

    const handleConnect = () => {
      clearTimeout(timeoutId!);
      timeoutId = setTimeout(() => {
        setConnected(true);
      }, 200);
    };

    const handleDisconnect = () => {
      setConnected(false);
      clearTimeout(timeoutId!);
    };

    activeSocket.on("connect", handleConnect);
    activeSocket.on("disconnect", handleDisconnect);

    return () => {
      activeSocket.off("connect", handleConnect);
      activeSocket.off("disconnect", handleDisconnect);
      clearTimeout(timeoutId!);
    };
  }, [generation]);

  return { connected, generation };
};
