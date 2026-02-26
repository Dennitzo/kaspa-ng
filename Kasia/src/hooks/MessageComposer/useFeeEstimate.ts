import { useEffect, useState } from "react";
import {
  Attachment,
  useComposerStore,
} from "../../store/message-composer.store";
import { useWalletStore } from "../../store/wallet.store";
import { Address } from "kaspa-wasm";
import { FeeState } from "../../types/all";

export const useFeeEstimate = ({
  toSelf = false,
  recipient,
  draft,
  attachment,
  broadcastOptions,
}: {
  toSelf?: boolean;
  recipient?: string;
  draft?: string;
  attachment?: Attachment;
  broadcastOptions?: {
    isBroadcast: boolean;
    channelName: string;
  };
}) => {
  const [feeState, setFeeState] = useState<FeeState>({ status: "idle" });

  const {
    priority,
    sendState: { status: sendStatus },
  } = useComposerStore();
  const {
    unlockedWallet,
    estimateSendMessageFees,
    estimateSendBroadcastFees,
    address,
    balance,
  } = useWalletStore();

  const matureBalance = balance?.mature;
  const addressString = address?.toString();
  const isBroadcast = broadcastOptions?.isBroadcast ?? false;
  const broadcastChannelName = broadcastOptions?.channelName ?? "";

  useEffect(() => {
    // when toSelf is true, we need user's address; otherwise we need recipient
    const targetAddress = toSelf ? addressString : recipient;

    if (
      !targetAddress ||
      (!draft && !attachment) ||
      !unlockedWallet ||
      sendStatus === "loading"
    ) {
      setFeeState({ status: "idle" });
      return;
    }

    // For broadcasts, we need channel options
    if (isBroadcast && !broadcastChannelName) {
      setFeeState({ status: "idle" });
      return;
    }

    // check if we have funds available
    if (!matureBalance || matureBalance === 0n) {
      setFeeState({
        status: "error",
        error: new Error("No funds available for fee estimation"),
      });
      return;
    }

    let parsedAddress: Address;
    try {
      parsedAddress = new Address(targetAddress);
    } catch {
      setFeeState({
        status: "error",
        error: new Error("Invalid address"),
      });
      return;
    }

    setFeeState({ status: "loading" });
    let isCancelled = false;

    // debounce the fee estimation
    const timeoutId = setTimeout(() => {
      // use attachment content if available, otherwise use draft text
      const messageContent = attachment ? attachment.content : draft || "";

      const estimatePromise = isBroadcast
        ? estimateSendBroadcastFees(
            messageContent,
            parsedAddress,
            broadcastChannelName,
            priority
          )
        : estimateSendMessageFees(messageContent, parsedAddress, priority);

      estimatePromise
        .then((estimate) => {
          if (!isCancelled) {
            const fee = Number(estimate.fees) / 100_000_000;
            setFeeState({ status: "idle", value: fee });
          }
        })
        .catch((error) => {
          if (!isCancelled) {
            setFeeState({ status: "error", error: error as Error });
          }
        });
    }, 400);

    return () => {
      // prevent promise from updating state after cleanup
      isCancelled = true;
      clearTimeout(timeoutId);
    };
  }, [
    toSelf,
    recipient,
    addressString,
    draft,
    attachment,
    isBroadcast,
    broadcastChannelName,
    priority,
    sendStatus,
    unlockedWallet,
    estimateSendMessageFees,
    estimateSendBroadcastFees,
    matureBalance,
  ]);

  return feeState;
};
