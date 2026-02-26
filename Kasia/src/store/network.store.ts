import { create } from "zustand";
import { NetworkType } from "../types/all";
import { ConnectStrategy, Encoding, Resolver, RpcClient } from "kaspa-wasm";
import { unknownErrorToErrorLike } from "../utils/errors";

interface NetworkState {
  isConnected: boolean;
  isConnecting: boolean;
  network: NetworkType;
  rpc: RpcClient;
  nodeUrl: string | null;

  /**
   * requires a call to `.connect(network: NetworkType)` if you want changes to be applied
   *
   * note: this persist the node url on the local storage and will be re-used on next startup
   */
  setNodeUrl: (url: string | null) => void;
  /**
   * requires a call to `.connect(network: NetworkType)` if you want changes to be applied
   */
  setNetwork: (network: NetworkType) => void;

  connect: () => Promise<void>;
  disconnect: () => Promise<void>;
}

/**
 * Responsability: manage Kaspa RPC connection
 */
export const useNetworkStore = create<NetworkState>((set, g) => {
  const initialNetwork =
    import.meta.env.VITE_DEFAULT_KASPA_NETWORK ?? "mainnet";
  const userInitialNodeUrl =
    localStorage.getItem(`kasia_node_url_${initialNetwork}`) ?? null;

  const onConnectionLost = async () => {
    const { rpc, connect } = g();

    // remove auto-reconnect
    rpc.removeEventListener("disconnect", onConnectionLost);

    console.warn("RPC connection lost. Attempting to reconnect...");

    try {
      await connect();
      console.log("Reconnected successfully.");
    } catch (error) {
      const errorAsError = unknownErrorToErrorLike(error);

      if (errorAsError.message.includes("Already connected")) {
        // ignore if already re-connected
        return;
      }
      console.error("Failed to reconnect:", errorAsError);

      // re-plan in 2 seconds
      setTimeout(onConnectionLost, 2_000);
    }
  };

  return {
    isConnected: false,
    isConnecting: false,
    network: initialNetwork,
    nodeUrl: userInitialNodeUrl,
    rpc: new RpcClient({
      encoding: Encoding.Borsh,
      networkId: initialNetwork,
    }),
    async connect(): Promise<void> {
      const rpc = g().rpc;

      const isDifferentNetwork = rpc.networkId?.toString() !== g().network;
      const isDifferentUrl = rpc?.url !== (g().nodeUrl ?? undefined);

      if (!isDifferentNetwork && !isDifferentUrl && rpc.isConnected) {
        console.warn("Trying to connect RPC while it is already connected.");
        throw new Error("Already connected.");
      }

      set({
        isConnecting: true,
        isConnected: false,
      });

      // remove auto-reconnect
      rpc.removeEventListener("disconnect", onConnectionLost);

      await rpc.disconnect();

      if (isDifferentNetwork) {
        rpc.setNetworkId(g().network);
      }

      const runtimeConfig =
        (globalThis as {
          __KASPA_NG_KASIA_CONFIG?: {
            defaultMainnetNodeUrl?: string;
            defaultTestnetNodeUrl?: string;
          };
        }).__KASPA_NG_KASIA_CONFIG ?? {};

      const kasiaNodeUrl =
        rpc.networkId?.toString() === "mainnet"
          ? runtimeConfig.defaultMainnetNodeUrl ??
            import.meta.env.VITE_DEFAULT_MAINNET_KASPA_NODE_URL
          : runtimeConfig.defaultTestnetNodeUrl ??
            import.meta.env.VITE_DEFAULT_TESTNET_KASPA_NODE_URL;

      try {
        await rpc.connect({
          blockAsyncConnect: true,
          retryInterval: 2_000,
          timeoutDuration: 3_000,
          url: g().nodeUrl ?? kasiaNodeUrl,
          strategy: ConnectStrategy.Fallback,
        });

        // verify network matches
        const serverInfo = await rpc.getServerInfo();

        if (serverInfo.networkId !== rpc.networkId?.toString()) {
          throw new Error(
            `RPC Node isn't on the correct networkId. Expected: ${rpc.networkId?.toString()}, Got: ${serverInfo.networkId}`
          );
        }

        // register auto-reconnect
        rpc.addEventListener("disconnect", onConnectionLost);

        // persist the nodeUrl uppon successful connection
        if (g().nodeUrl) {
          localStorage.setItem(
            `kasia_node_url_${rpc.networkId?.toString()}`,
            g().nodeUrl ?? ""
          );
        } else {
          localStorage.removeItem(
            `kasia_node_url_${rpc.networkId?.toString()}`
          );
        }

        set({
          isConnected: true,
          isConnecting: false,
        });
        return;
      } catch (error) {
        set({
          isConnecting: false,
        });
        throw error;
      }
    },
    async disconnect() {
      const rpc = g().rpc;
      if (rpc.isConnected) {
        // remove auto-reconnect as this is an explicit disconnection
        rpc.removeEventListener("disconnect", onConnectionLost);

        await rpc.disconnect();
        set({ isConnected: false });
      }
    },
    setNetwork(network) {
      const nodeUrl = localStorage.getItem(`kasia_node_url_${network}`) ?? null;

      set({ network, nodeUrl });
    },
    setNodeUrl(nodeUrl) {
      set({ nodeUrl });
    },
  };
});
