import { FC, useRef, useEffect } from "react";
import { ChevronLeft, AlertTriangle } from "lucide-react";
import { useBroadcastStore } from "../../store/broadcast.store";
import { useWalletStore } from "../../store/wallet.store";
import { useIsMobile } from "../../hooks/useIsMobile";
import { BroadcastComposer } from "./Composing/Broadcast/BroadcastComposer";
import { BroadcastMessagesList } from "./Broadcasts/BroadcastMessagesList";

export const BroadcastSection: FC<{
  mobileView: "contacts" | "messages";
  setMobileView: (v: "contacts" | "messages") => void;
}> = ({ mobileView, setMobileView }) => {
  const isMobile = useIsMobile();
  const { address: walletAddress } = useWalletStore();
  const messagesScrollRef = useRef<HTMLDivElement>(null);

  const selectedChannelName = useBroadcastStore((s) => s.selectedChannelName);

  const channels = useBroadcastStore((s) => s.channels);
  const messages = useBroadcastStore((s) => s.messages);

  // get the selected channel data
  const selectedChannel = selectedChannelName
    ? channels.find((channel) => channel.channelName === selectedChannelName)
    : null;

  // get messages for the selected channel
  const channelMessages = selectedChannelName
    ? messages
        .filter((msg) => msg.channelName === selectedChannelName.toLowerCase())
        .sort((a, b) => a.timestamp.getTime() - b.timestamp.getTime())
    : [];

  const scrollToBottom = () => {
    const el = messagesScrollRef.current;
    if (el) {
      el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
    }
  };

  // scroll to bottom when channel is selected or new messages arrive
  useEffect(() => {
    if (messagesScrollRef.current && selectedChannelName) {
      // delay scroll to account for keyboard appearing after autofocus
      if (isMobile) {
        setTimeout(() => scrollToBottom(), 300);
      } else {
        scrollToBottom();
      }
    }
  }, [selectedChannelName, messages.length, isMobile]);

  if (!selectedChannelName || !selectedChannel) {
    return (
      <div
        className={`flex flex-[2] flex-col overflow-x-hidden ${isMobile ? "" : "border-primary-border border-l"} ${isMobile && mobileView === "contacts" ? "hidden" : ""}`}
      >
        {/* Header */}
        <div className="flex h-[60px] items-center justify-between bg-[var(--secondary-bg)] px-4">
          <div className="flex items-center">
            <button
              onClick={() => {
                setMobileView("contacts");
              }}
              className="mr-2 cursor-pointer p-1 sm:hidden"
              aria-label="Back to channels"
            >
              <ChevronLeft className="size-6" />
            </button>
            <h3 className="flex items-center gap-2 truncate text-base font-semibold sm:hidden">
              Broadcast Messages
            </h3>
          </div>
        </div>

        {/* Empty state */}
        <div className="bg-primary-bg flex-1 overflow-x-hidden overflow-y-auto px-1 py-4 pb-8 sm:px-2">
          <div className="m-5 rounded-[12px] bg-[rgba(0,0,0,0.2)] px-5 py-10 text-center text-[var(--text-secondary)] italic">
            Select a broadcast channel to view messages...
          </div>
        </div>
      </div>
    );
  }

  return (
    <div
      className={`flex flex-[2] flex-col overflow-x-hidden ${isMobile ? "" : "border-primary-border border-l"} ${isMobile && mobileView === "contacts" ? "hidden" : ""}`}
    >
      {/* Channel header */}
      <div className="flex h-[60px] items-center justify-between bg-[var(--secondary-bg)] px-4">
        <div className="flex min-w-0 flex-1 items-center">
          <button
            onClick={() => {
              setMobileView("contacts");
            }}
            className="mr-2 cursor-pointer p-1 sm:hidden"
            aria-label="Back to channels"
          >
            <ChevronLeft className="size-6" />
          </button>
          <h3
            className="truncate text-base font-semibold"
            title={`# ${selectedChannel?.channelName}`}
          >
            # {selectedChannel?.channelName}
          </h3>
        </div>
        <div className="text-accent-yellow flex items-center gap-2 text-right">
          <span className="text-sm font-semibold sm:text-base">
            Unencrypted
          </span>
          <AlertTriangle className="size-6 flex-shrink-0 sm:size-7" />
        </div>
      </div>

      {/* Messages area */}
      <div
        className="bg-primary-bg flex flex-1 flex-col overflow-x-hidden overflow-y-auto px-1 py-4 pb-8 sm:px-2"
        ref={messagesScrollRef}
      >
        <BroadcastMessagesList
          messages={channelMessages}
          walletAddress={walletAddress?.toString() || ""}
        />
      </div>

      {/* Broadcast composer */}
      <BroadcastComposer channelName={selectedChannel?.channelName || ""} />
    </div>
  );
};
