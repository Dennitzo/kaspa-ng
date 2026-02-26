import clsx from "clsx";
import { formatKasAmount } from "../../../../utils/format";
import { PriorityFeeSelector } from "../../../PriorityFeeSelector";
import { PriorityFeeConfig } from "../../../../types/all";
import { Attachment } from "../../../../store/message-composer.store";
import { FeeState } from "../../../../types/all";
import { useWalletStore } from "../../../../store/wallet.store";
import { WaitDisplay } from "./WaitDisplay";

// fee levels for color coding
// need to extract this and make it setable from the settings
const FEE_LEVELS = [
  {
    limit: 0.00002,
    classes: "text-[var(--accent-green)] border-[var(--accent-green)]",
  },
  {
    limit: 0.00005,
    classes: "text-[var(--accent-blue)] border-[var(--accent-blue)]",
  },
  {
    limit: 0.0005,
    classes: "text-[var(--accent-yellow)] border-[var(--accent-yellow)]",
  },
  {
    limit: 0.001,
    classes: "text-[var(--accent-orange)] border-[var(--accent-orange)]",
  },
  {
    limit: Infinity,
    classes: "text-[var(--accent-red)] border-[var(--accent-red)]",
  },
];

function getFeeClasses(fee: number) {
  return FEE_LEVELS.find(({ limit }) => fee <= limit)!.classes;
}

interface FeeDisplayProps {
  draft?: string;
  attachment?: Attachment | null;
  feeState: FeeState;
  priority: PriorityFeeConfig;
  onPriorityChange: (priority: PriorityFeeConfig) => void;
}

// this displayes the fee above the message box and colors it!
export const FeeDisplay = ({
  draft,
  attachment,
  feeState,
  priority,
  onPriorityChange,
}: FeeDisplayProps) => {
  const balance = useWalletStore((state) => state.balance);
  // show fee display when we have content to send: draft or attachment
  if (!draft && !attachment) {
    return null;
  }

  // check if there are funds available
  const hasFunds = balance && balance.mature > 0n;

  return (
    <div className="absolute -top-7.5 right-4 flex items-center gap-2">
      <WaitDisplay estimatedSeconds={priority.estimatedSeconds} />
      <div
        className={clsx(
          "inline-block rounded-md border bg-[var(--secondary-bg)]/20 px-3 py-1 text-right text-xs transition-opacity duration-300 ease-out",
          feeState.value
            ? getFeeClasses(feeState.value)
            : "text-[var(--text-secondary)]"
        )}
      >
        {!hasFunds
          ? "No funds available"
          : feeState.status === "loading"
            ? feeState.value != null
              ? `Updating fee… ${formatKasAmount(feeState.value)} KAS`
              : "Estimating fee…"
            : feeState.value != null
              ? `Estimated fee: ${formatKasAmount(feeState.value)} KAS`
              : feeState.status === "error"
                ? "Fee estimation failed"
                : "Calculating fee…"}
      </div>
      <PriorityFeeSelector
        currentFee={priority}
        onFeeChange={onPriorityChange}
        className="mr-0 sm:mr-2"
      />
    </div>
  );
};
