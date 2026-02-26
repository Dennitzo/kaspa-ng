import { FC, useEffect, useState } from "react";
import { decryptXChaCha20Poly1305 } from "kaspa-wasm";
import { useWalletStore } from "../../store/wallet.store";
import { StoredWallet } from "../../types/wallet.type";
import { Eye, EyeOff, Lock } from "lucide-react";
import clsx from "clsx";
import { Button } from "../Common/Button";
import { toast } from "../../utils/toast-helper";
import { StringCopy } from "../Common/StringCopy";
import { WarningBlock } from "../Common/WarningBlock";
import { PasswordField } from "../Common/PasswordField";

export const WalletSeedRetreiveDisplay: FC = () => {
  const [password, setPassword] = useState("");
  const [showSeedPhrase, setShowSeedPhrase] = useState(false);
  const [seedPhrase, setSeedPhrase] = useState("");
  const [isBlurred, setIsBlurred] = useState(true);
  const [hasPassphrase, setHasPassphrase] = useState(false);
  const selectedWalletId = useWalletStore((state) => state.selectedWalletId);
  const [blurTimeout, setBlurTimeout] = useState<NodeJS.Timeout | null>(null);

  // Clear timeout on unmount
  useEffect(() => {
    return () => {
      if (blurTimeout) {
        clearTimeout(blurTimeout);
      }
    };
  }, [blurTimeout]);

  const handleBlurToggle = (shouldBlur: boolean) => {
    // Clear any existing timeout
    if (blurTimeout) {
      clearTimeout(blurTimeout);
      setBlurTimeout(null);
    }

    setIsBlurred(shouldBlur);

    // If unblurring, set a timeout to re-blur after 5 seconds
    if (!shouldBlur) {
      setBlurTimeout(
        setTimeout(() => {
          setIsBlurred(true);
        }, 5000)
      );
    }
  };

  const handleViewSeedPhrase = async () => {
    try {
      if (!selectedWalletId) {
        toast.error("No wallet selected");
        return;
      }

      // Get the stored wallet data
      const walletsString = localStorage.getItem("wallets");
      if (!walletsString) {
        toast.error("No wallets found");
        return;
      }

      const storedWallets: StoredWallet[] = JSON.parse(walletsString);
      const foundStoredWallet = storedWallets.find(
        (w) => w.id === selectedWalletId
      );
      if (!foundStoredWallet) {
        toast.error("Wallet not found");
        return;
      }

      // Check if wallet has a passphrase
      setHasPassphrase(!!foundStoredWallet.encryptedPassphrase);

      // Decrypt the seed phrase
      const phrase = decryptXChaCha20Poly1305(
        foundStoredWallet.encryptedPhrase,
        password
      );
      setSeedPhrase(phrase);
      setShowSeedPhrase(true);
    } catch (error) {
      console.error("Error viewing seed phrase:", error);
      toast.error("Invalid password");
    }
  };

  return (
    <div className="mt-2">
      <h4 className="text-center text-lg font-semibold">Security</h4>
      <WarningBlock title="Warning" className="mt-2">
        Never share your seed phrase with anyone. Anyone with access to your
        seed phrase can access your funds.
      </WarningBlock>
      {!showSeedPhrase ? (
        <div>
          <p className="mb-2 p-2 text-center font-semibold">
            Enter your password to view seed phrase:
          </p>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              handleViewSeedPhrase();
            }}
            className="space-y-4"
          >
            {/* hidden username field for password manager accessibility */}
            <input
              type="text"
              name="username"
              value="wallet-seed-access"
              autoComplete="username"
              style={{ display: "none" }}
              readOnly
              tabIndex={-1}
            />
            <PasswordField
              classInput="border-primary-border bg-primary-bg w-full rounded-3xl border px-4 py-2"
              onChange={(e) => setPassword(e.target.value)}
              placeholder="Enter wallet password"
              value={password}
              required
            />
            <Button type="submit" variant="primary" className="w-full">
              View Seed Phrase
            </Button>
          </form>
        </div>
      ) : (
        <div>
          {hasPassphrase && (
            <WarningBlock title="Extra Security" icon={Lock} className="my-2">
              This wallet was created with a BIP39 passphrase.
              <br />
              You'll need both the seed phrase AND the passphrase to recover
              this wallet elsewhere.
            </WarningBlock>
          )}
          <p className="mb-2 p-2 text-center font-semibold">
            Your seed phrase:
          </p>
          <div
            className={clsx(
              "word-break border-primary-border bg-primary-bg mb-4 rounded-3xl border px-4 py-3 font-mono break-all",
              { "blur-sm filter": isBlurred },
              { "select-none": isBlurred }
            )}
          >
            {seedPhrase}
          </div>
          <div className="flex items-center justify-center gap-2">
            <input
              type="checkbox"
              id="toggleVisibility"
              checked={!isBlurred}
              onChange={(e) => handleBlurToggle(!e.target.checked)}
              className="hidden"
            />
            <label htmlFor="toggleVisibility" className="mb-2 cursor-pointer">
              {isBlurred ? (
                <Eye className="h-6 w-6" />
              ) : (
                <EyeOff className="h-6 w-6" />
              )}
            </label>
            <StringCopy
              text={seedPhrase}
              alertText="Seed phrase copied to clipboard"
              titleText="Copy seed phrase"
              iconClass="h-6 w-6"
              className="mb-2 p-1"
            />
          </div>
          <div className="mt-4 flex justify-center">
            <Button
              onClick={() => {
                setShowSeedPhrase(false);
                setSeedPhrase("");
                setPassword("");
                setIsBlurred(true);
                setHasPassphrase(false);
              }}
              variant="secondary"
              className="px-3 py-2 shadow"
            >
              Hide Seed Phrase
            </Button>
          </div>
        </div>
      )}
    </div>
  );
};
