import React, { useCallback, useEffect } from "react";
import { useNetworkStore } from "./store/network.store";
import { useUiStore } from "./store/ui.store";
import type { NetworkType } from "./types/all";
import { AppRoutes } from "./AppRoutes";
import { useIsMobile } from "./hooks/useIsMobile";
import { syncThemeColorMeta } from "./utils/meta-theme-syncer";
import {
  applyCustomColors,
  resetCustomColors,
} from "./config/custom-theme-applier";
import { useOrchestrator } from "./hooks/useOrchestrator";
import { cleanupLegacyLocalStorage } from "./utils/storage-cleanup";
import { core } from "@tauri-apps/api";
import { listen } from "@tauri-apps/api/event";

const knownNonce: Set<number> = new Set();

const App: React.FC = () => {
  const networkStore = useNetworkStore();
  const { theme, getEffectiveTheme, customColors } = useUiStore();
  const { connect, onPause, onResume } = useOrchestrator();
  const isMobile = useIsMobile();

  // biome-ignore lint/correctness/useExhaustiveDependencies: We don't want re-trigger, this is init trigger
  useEffect(() => {
    const asyncDefer = async () => {
      await connect();

      // clean up legacy localStorage keys that stored wallet addresses
      cleanupLegacyLocalStorage();

      if (core.isTauri()) {
        // on pause and on resume registering
        // addPluginListener("app-events", "pause", onPause);
        // addPluginListener("app-events", "resume", onResume);

        listen<number>("paused", (event) => {
          const nonce = event.payload;

          console.log(`nonce ${nonce}, known? ${knownNonce.has(nonce)}`);

          if (knownNonce.has(nonce)) {
            return;
          }

          knownNonce.add(nonce);
          onPause();
        });

        listen<number>("resumed", (event) => {
          const nonce = event.payload;

          console.log(`nonce ${nonce}, known? ${knownNonce.has(nonce)}`);

          if (knownNonce.has(nonce)) {
            return;
          }

          knownNonce.add(nonce);
          onResume();
        });

        // nOnResume(onResume);
        // nOnPause(onPause);
      }
    };
    asyncDefer();
  }, []);

  const onNetworkChange = useCallback(
    (n: NetworkType) => {
      networkStore.setNetwork(n);
      connect({ networkType: n });
    },
    [connect, networkStore]
  );

  useEffect(() => {
    const meta = document.querySelector<HTMLMetaElement>(
      'meta[name="viewport"]'
    );
    if (!meta) return;
    meta.content = isMobile
      ? "width=device-width, initial-scale=1.0, viewport-fit=cover"
      : "width=device-width, initial-scale=1.0, viewport-fit=cover";
  }, [isMobile]);

  // Initialize theme and listen for system changes
  useEffect(() => {
    const effectiveTheme = getEffectiveTheme();
    document.documentElement.setAttribute("data-theme", effectiveTheme);

    // sync the theme color meta tag
    syncThemeColorMeta();

    // Listen for system theme changes when using "system" mode
    const mediaQuery = window.matchMedia("(prefers-color-scheme: light)");
    const handleSystemThemeChange = () => {
      if (theme === "system") {
        const newEffectiveTheme = getEffectiveTheme();
        document.documentElement.setAttribute("data-theme", newEffectiveTheme);
        syncThemeColorMeta(); // update meta tag if system theme changes
      }
    };

    mediaQuery.addEventListener("change", handleSystemThemeChange);

    return () => {
      mediaQuery.removeEventListener("change", handleSystemThemeChange);
    };
  }, [theme, getEffectiveTheme]);

  // Apply custom colors only when custom theme is selected
  useEffect(() => {
    if (theme === "custom" && customColors) {
      applyCustomColors(customColors);
    } else if (theme !== "custom") {
      // reset custom colors when switching away from custom theme
      resetCustomColors();
    }
  }, [theme, customColors]);

  return (
    <AppRoutes
      network={networkStore.network}
      isConnected={networkStore.isConnected}
      isConnecting={networkStore.isConnecting}
      onNetworkChange={onNetworkChange}
    />
  );
};

export default App;
