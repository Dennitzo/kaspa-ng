import { FC } from "react";
import { Outlet, useNavigate } from "react-router";
import { useWalletStore } from "../../store/wallet.store";
import { useIsMobile } from "../../hooks/useIsMobile";
import { Header } from "../Layout/Header";
import { SlideOutMenu } from "../Layout/SlideOutMenu";

import { ToastContainer } from "../Common/ToastContainer";
import { useUiStore } from "../../store/ui.store";
import { ResizableAppContainer } from "./ResizableAppContainer";
import { ModalHost } from "./ModalHost";
import { ConnectionIndicator } from "../Common/ConnectionIndicator";

export const RootLayout: FC = () => {
  const walletStore = useWalletStore();
  const uiStore = useUiStore();

  const isWalletReady = Boolean(walletStore.unlockedWallet);
  const isMobile = useIsMobile();

  const navigate = useNavigate();

  const handleCloseWallet = () => {
    // close settings panel
    uiStore.setSettingsOpen(false);

    // navigate home
    navigate("/");
  };

  return (
    <>
      <ToastContainer />
      <ModalHost />
      <ResizableAppContainer>
        {/* desktop header */}
        {!isMobile && (
          <Header
            isWalletReady={isWalletReady}
            walletAddress={walletStore.address?.toString()}
            onCloseWallet={handleCloseWallet}
          />
        )}

        {/* mobile drawer */}
        {isMobile && (
          <>
            <div className="absolute top-4 left-1/2 z-100 -translate-x-1/2">
              <ConnectionIndicator />
            </div>
            <SlideOutMenu
              isWalletReady={isWalletReady}
              address={walletStore.address?.toString()}
              onCloseWallet={handleCloseWallet}
            />
          </>
        )}

        <Outlet />
      </ResizableAppContainer>
    </>
  );
};
