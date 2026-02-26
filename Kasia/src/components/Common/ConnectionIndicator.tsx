import { useEffect, useState } from "react";
import { useNetworkStore } from "../../store/network.store";
import { useWalletStore } from "../../store/wallet.store";
import { useMessagingStore } from "../../store/messaging.store";
import { WifiOff, Wifi } from "lucide-react";
import clsx from "clsx";

const CONNECTED_FADE_TIME = 5000;

export const ConnectionIndicator = () => {
  const networkStore = useNetworkStore();
  const walletStore = useWalletStore();
  const messagingStore = useMessagingStore();
  const [isVisible, setIsVisible] = useState(false);
  const [isReady, setIsReady] = useState(false);

  const isWalletReady = walletStore.unlockedWallet && messagingStore.isLoaded;

  // delay showing the indicator after wallet unlocks
  useEffect(() => {
    if (isWalletReady) {
      const readyTimer = setTimeout(
        () => setIsReady(true),
        CONNECTED_FADE_TIME + 1000
      );
      return () => clearTimeout(readyTimer);
    } else {
      setIsReady(false);
    }
  }, [isWalletReady]);

  // when connected, hide after some time
  useEffect(() => {
    if (networkStore.isConnected && isReady) {
      setIsVisible(true);

      const timer = setTimeout(() => setIsVisible(false), CONNECTED_FADE_TIME);
      return () => clearTimeout(timer);
    } else {
      setIsVisible(false);
    }
  }, [networkStore.isConnected, isReady]);

  // don't use until unlocked and loaded, and ready period has passed
  if (!isWalletReady || !isReady) return null;

  // show yellow wifi-off icon when reconnecting
  if (!networkStore.isConnected && networkStore.isConnecting) {
    return (
      <WifiOff
        className={clsx(
          "size-9 animate-pulse text-[var(--accent-yellow)] sm:size-6"
        )}
      />
    );
  }

  // show red wifi-off icon when disconnected and not reconnecting
  if (!networkStore.isConnected) {
    return (
      <WifiOff
        className={clsx(
          "size-9 animate-pulse text-[var(--accent-red)] sm:size-6"
        )}
      />
    );
  }

  // show green wifi icon when connected and within fade period, otherwise nothing
  return isVisible ? (
    <Wifi
      className={clsx(
        "animate-fade-out-5s size-9 text-[var(--accent-green)] sm:size-6"
      )}
    />
  ) : null;
};
