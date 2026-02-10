import QrCodeModal from "./QrCodeModal";
import Tooltip, { TooltipDisplayMode } from "./Tooltip";
import Copy from "./assets/copy.svg";
import CopyCheck from "./assets/copycheck.svg";
import QrCode from "./assets/qr_code.svg";
import { AddressLabelContext } from "./context/AddressLabelProvider";
import { useContext, useState } from "react";
import { Link } from "react-router";

interface KasLinkProps {
  linkType: "transaction" | "block" | "address";
  linkAdditionalParams?: string;
  to: string;
  className?: string;
  copy?: boolean;
  qr?: boolean;
  link?: boolean;
  active?: boolean;
  shorten?: boolean;
  resolveName?: boolean;
  mono?: boolean;
  newTab?: boolean;
}

const linkTypeToAddress: Record<KasLinkProps["linkType"], string> = {
  transaction: "/transactions/",
  block: "/blocks/",
  address: "/addresses/",
};

const KasLink = ({ to, linkType, copy, qr, link, shorten, resolveName, mono, newTab, className }: KasLinkProps) => {
  const [clicked, setClicked] = useState(false);
  const [showQr, setShowQr] = useState(false);
  const isAddress = linkType === "address";
  const normalizedAddress =
    isAddress && to && !to.startsWith("kaspa:") && !to.startsWith("kaspatest:")
      ? `kaspa:${to}`
      : to;
  const linkHref = linkTypeToAddress[linkType] + (isAddress ? normalizedAddress : to);
  const { labels } = useContext(AddressLabelContext);
  const resolveAddressLabel = () => {
    if (linkType !== "address") return "";
    if (labels[to]) return labels[to];
    if (to.startsWith("kaspa:")) {
      const raw = to.split(":", 2)[1];
      return labels[raw] || "";
    }
    return labels[`kaspa:${to}`] || "";
  };
  const label = resolveAddressLabel();

  const handleClick = async () => {
    const markCopied = () => {
      setClicked(true);
      setTimeout(() => setClicked(false), 1000);
    };

    try {
      if (navigator.clipboard && window.isSecureContext) {
        await navigator.clipboard.writeText(to);
        markCopied();
        return;
      }
    } catch {
      // Fallback to execCommand below.
    }

    try {
      const textarea = document.createElement("textarea");
      textarea.value = to;
      textarea.setAttribute("readonly", "");
      textarea.style.position = "fixed";
      textarea.style.top = "-1000px";
      textarea.style.left = "-1000px";
      document.body.appendChild(textarea);
      textarea.select();
      const success = document.execCommand("copy");
      document.body.removeChild(textarea);
      if (success) {
        markCopied();
      }
    } catch {
      // No-op; Safari may block clipboard in some contexts.
    }
  };

  if (!to) {
    return <></>;
  }

  const splitAt = linkType === "address" ? 13 : 8;
  let displayValue: string | React.ReactNode = shorten
    ? to.substring(0, splitAt) + "…" + to.substring(to.length - 8)
    : to;

  if (linkType === "address" && resolveName) {
    displayValue = (
      <Tooltip message={to} display={TooltipDisplayMode.Hover}>
        {displayValue}
      </Tooltip>
    );
  }

  return (
    <span>
      <span className={"break-all " + (mono ? "font-mono tracking-tighter " : "") + (className || "")}>
        {link && linkHref ? (
          <Link
            className="text-link hover:underline"
            to={linkHref}
            target={newTab ? "_blank" : undefined}
            rel={newTab ? "noopener noreferrer" : undefined}
          >
            {displayValue}
          </Link>
        ) : (
          displayValue
        )}
      </span>
      {label && (
        <Tooltip message={"Address Label"} display={TooltipDisplayMode.Hover}>
          <span
            className="ml-2 inline-block rounded-full border px-2 py-0.5 text-xs font-medium text-nowrap"
            style={{ borderColor: "#70C7BA", color: "#70C7BA", backgroundColor: "transparent" }}
          >
            {label}
          </span>
        </Tooltip>
      )}
      <span className="fill-gray-500">
        <Tooltip message={"Copied"} display={TooltipDisplayMode.Click} clickTimeout={1000}>
          {copy &&
            (!clicked ? (
              <Copy
                className="hover:fill-primary mx-1 inline h-4 w-4 align-middle hover:cursor-pointer"
                onClick={handleClick}
              />
            ) : (
              <CopyCheck className="mx-1 inline h-4 w-4 align-middle" />
            ))}
        </Tooltip>

        {qr && (
          <QrCode
            className="hover:fill-primary relative inline h-4 w-4 align-middle hover:cursor-pointer"
            onClick={() => setShowQr(!showQr)}
          />
        )}
      </span>
      {showQr && <QrCodeModal value={to} setShowQr={setShowQr} />}
    </span>
  );
};

export default KasLink;
