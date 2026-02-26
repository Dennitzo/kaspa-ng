import { useState, useEffect } from "react";
import { useIsMobile } from "./useIsMobile";

export const useMobileViewManager = (
  contactId: string | undefined,
  channelId: string | undefined,
  isCurrentlyInBroadcastMode: boolean,
  messageStoreLoaded: boolean
) => {
  const [mobileView, setMobileView] = useState<"contacts" | "messages">(
    "contacts"
  );
  const isMobile = useIsMobile();

  // effect to handle if you drag from desktop to mobile, we need the mobile view to be aware!
  useEffect(() => {
    const syncToWidth = () => {
      if (isMobile) {
        // on mobile, show messages if there's an opened conversation or broadcast channel (based on url)
        const hasOpenedConversation = contactId;
        const hasOpenedBroadcast = isCurrentlyInBroadcastMode && channelId;

        if (hasOpenedConversation || hasOpenedBroadcast) {
          setMobileView("messages");
        } else {
          setMobileView("contacts");
        }
      } else {
        setMobileView("contacts");
      }
    };

    syncToWidth(); // run once on mount
    window.addEventListener("resize", syncToWidth);
    return () => window.removeEventListener("resize", syncToWidth);
  }, [contactId, channelId, isMobile, isCurrentlyInBroadcastMode]);

  // effect to update mobile view when URL changes (contact/channel selection)
  useEffect(() => {
    if (isMobile && messageStoreLoaded) {
      const hasOpenedConversation = contactId;
      const hasOpenedBroadcast = isCurrentlyInBroadcastMode && channelId;

      if (hasOpenedConversation || hasOpenedBroadcast) {
        setMobileView("messages");
      } else {
        setMobileView("contacts");
      }
    }
  }, [
    isMobile,
    contactId,
    channelId,
    messageStoreLoaded,
    isCurrentlyInBroadcastMode,
  ]);

  return { mobileView, setMobileView, isMobile };
};
