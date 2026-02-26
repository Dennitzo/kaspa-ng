// src/components/CopyableValueWithQR.tsx
import { FC, useState, useEffect } from "react";
import { toDataURL } from "qrcode";
import { Copy, QrCode } from "lucide-react";
import { Button } from "../Common/Button";
import { toast } from "../../utils/toast-helper";
import { copyToClipboard } from "../../utils/clipboard";

type CopyableValueWithQRProps = {
  value?: string;
  label?: string;
  qrTitle?: string;
  onQrToggle?: (isOpen: boolean) => void;
};

export const CopyableValueWithQR: FC<CopyableValueWithQRProps> = ({
  value = "",
  label = "",
  qrTitle = "",
  onQrToggle,
}) => {
  const [showQRCode, setShowQRCode] = useState(false);
  const [qrCodeURL, setQRCodeURL] = useState<string | undefined>(undefined);

  useEffect(() => {
    if (!value) return;
    toDataURL(value, (err, uri) => {
      if (!err) {
        setQRCodeURL(uri);
        console.log("QR code generated successfully");
      }
    });
  }, [value]);

  const handleCopyValue = async () => {
    if (!value) {
      toast.error("No value available");
      console.log("No value to copy");
      return;
    }
    console.log("Copying value to clipboard");
    await copyToClipboard(value, "Copied to clipboard");
    console.log("Value copied successfully");
  };

  const toggleQRCode = () => {
    setShowQRCode((prev) => {
      const newState = !prev;
      onQrToggle?.(newState);
      console.log("QR code visibility toggled");
      return newState;
    });
  };

  if (!value) return null;
  return (
    <div className="relative w-full">
      {label && <strong>{label}</strong>}
      <div className="string-actions my-1 flex flex-col items-center gap-2 sm:flex-row">
        <div className="flex w-full">
          <span
            id="string-value"
            className="border-primary-border bg-primary-bg flex min-h-14 w-full cursor-pointer items-center rounded-lg border px-3 py-2 font-mono text-[13px] leading-[1.4] break-all transition-colors"
            onClick={() => {
              // Select the text when clicked
              const selection = window.getSelection();
              const range = document.createRange();
              const valueElement = document.getElementById("string-value");
              if (valueElement && selection) {
                range.selectNodeContents(valueElement);
                selection.removeAllRanges();
                selection.addRange(range);
              }
            }}
            title="Click to select value"
          >
            {value}
          </span>
        </div>
        <div className="flex w-full items-center justify-between gap-2 sm:w-auto sm:justify-start">
          <Button
            onClick={async (e) => {
              e.preventDefault();
              e.stopPropagation();
              console.log("Copy button clicked");
              handleCopyValue();
            }}
            title="Copy value to clipboard"
            type="button"
            variant="primary"
            className="flex h-14 w-full items-center justify-center rounded-xl p-0"
          >
            <Copy className="h-8 w-8 sm:h-6 sm:w-6" />
          </Button>
          <Button
            onClick={toggleQRCode}
            title="Show QR code"
            type="button"
            variant="primary"
            className="flex h-14 w-full items-center justify-center rounded-xl p-0"
          >
            <QrCode className="h-8 w-8 sm:h-6 sm:w-6" />
          </Button>
        </div>
      </div>

      <div
        className={`overflow-hidden transition-all duration-300 ease-in-out ${
          showQRCode && qrCodeURL
            ? "mt-4 max-h-96 opacity-100"
            : "max-h-0 opacity-0"
        }`}
      >
        <div className="border-primary-border bg-primary-bg w-full flex-col items-center rounded-lg border p-4 not-even:flex sm:w-auto">
          <h4 className="text-center">{qrTitle}</h4>
          <div className="flex flex-col items-center gap-2">
            <img
              src={qrCodeURL}
              alt={`QR Code for ${qrTitle.toLowerCase()}`}
              className="bg-primary-bg h-auto w-full max-w-[350px] min-w-[200px] rounded-lg p-2"
              onLoad={() => console.log("QR code image loaded successfully")}
              onError={(e) => {
                console.error("QR code image failed to load:", e);
                console.log("Failed URL:", qrCodeURL);
              }}
            />
          </div>
        </div>
      </div>
    </div>
  );
};
