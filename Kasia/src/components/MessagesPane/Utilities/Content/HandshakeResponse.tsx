import React, { useState } from "react";
import { useMessagingStore } from "../../../../store/messaging.store";
import { useWalletStore } from "../../../../store/wallet.store";
import { kaspaToSompi } from "kaspa-wasm";
import {
  PendingConversation,
  RejectedConversation,
} from "../../../../store/repository/conversation.repository";
import { Contact } from "../../../../store/repository/contact.repository";

export const HandshakeResponse: React.FC<{
  conversation: PendingConversation | RejectedConversation;
  contact: Contact;
  handshakeId: string;
}> = ({ conversation, contact, handshakeId }) => {
  const messagingStore = useMessagingStore();
  const walletBalance = useWalletStore((s) => s.balance);
  const [isResponding, setIsResponding] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Temp minimum balance to respond.
  // TODO: Refine this, potentially just have a wallet store function that has balance setpoints and return a bool
  // Can then use this in other parts of the app (such as minimum to send message etc)
  const minBalance = kaspaToSompi("0.4");
  const totalBalance =
    walletBalance?.mature && walletBalance?.pending
      ? walletBalance.mature + walletBalance.pending
      : walletBalance?.mature || BigInt(0);
  const hasInsufficientBalance =
    !totalBalance || !minBalance || totalBalance < minBalance;
  const hasPendingFunds =
    walletBalance?.pending && walletBalance.pending > BigInt(0);

  const handleRespond = async () => {
    try {
      setIsResponding(true);
      setError(null);
      await messagingStore.respondToHandshake(handshakeId);
    } catch (error) {
      console.error("Error responding to handshake:", error);
      setError(
        error instanceof Error ? error.message : "Failed to send response"
      );
    } finally {
      setIsResponding(false);
    }
  };

  const shouldShowButton =
    !conversation.initiatedByMe && conversation.status === "pending";

  return (
    <div className="my-2 rounded-lg">
      <div className="flex">
        <div className="mb-1 flex-1">
          <p className="mt-1 font-semibold text-[var(--text-secondary)]">
            Handshake received from:
          </p>
          <p className="ml-2 text-sm font-semibold break-all text-[var(--text-primary)]">
            {contact.kaspaAddress}
          </p>
          <p className="mt-1 text-sm font-semibold text-[var(--text-secondary)]">
            Alias:{" "}
            <span className="ml-2 font-normal text-[var(--text-primary)]">
              {conversation.theirAlias}
            </span>
          </p>
          <p className="font-semibold text-[var(--text-secondary)]">Status:</p>
          <p className="mb-0.5 ml-2 text-base font-semibold text-[var(--text-primary)] capitalize">
            {conversation.status}
          </p>
          {error && <p className="mt-2 text-[var(--accent-red)]">{error}</p>}
        </div>
        <div className="ml-2 flex flex-col items-center justify-center select-none">
          <img
            src="/kasia-logo.png"
            alt="Kasia Logo"
            className="h-32 w-32 object-contain opacity-60"
          />
        </div>
      </div>
      {shouldShowButton && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            handleRespond();
          }}
          className="cursor-pointer rounded border-none bg-[var(--button-primary)] px-4 py-2 text-sm text-[var(--text-primary)] transition-colors duration-200 hover:bg-[var(--button-primary)]/80 disabled:cursor-not-allowed disabled:bg-[var(--primary-border)]/80"
          disabled={isResponding || hasInsufficientBalance}
        >
          {isResponding
            ? "Sending Response..."
            : hasInsufficientBalance
              ? hasPendingFunds
                ? "Pending Incoming Funds"
                : "Insufficient Funds To Respond"
              : "Accept & Send Response"}
        </button>
      )}
    </div>
  );
};
