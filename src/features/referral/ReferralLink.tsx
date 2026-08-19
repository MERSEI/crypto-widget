import { useEffect, useState } from "react";
import { useReferralStore } from "../../core/store/referral";
import { QrCode } from "./QrCode";

interface Props {
  link: string;
}

/** Clipboard access can be refused by the webview; falling back keeps Copy working rather
 *  than failing silently, which for a link the user is about to paste somewhere matters. */
async function copyToClipboard(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    const field = document.createElement("textarea");
    field.value = text;
    field.style.position = "fixed";
    field.style.opacity = "0";
    document.body.appendChild(field);
    field.select();
    const copied = document.execCommand("copy");
    document.body.removeChild(field);
    return copied;
  }
}

export function ReferralLink({ link }: Props) {
  const openUrl = useReferralStore((s) => s.openUrl);
  const [copied, setCopied] = useState(false);
  const [showQr, setShowQr] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!copied) return;
    const id = setTimeout(() => setCopied(false), 1600);
    return () => clearTimeout(id);
  }, [copied]);

  const handleCopy = async () => {
    setCopied(await copyToClipboard(link));
  };

  const handleOpen = async () => {
    try {
      setError(null);
      await openUrl(link);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="referral-link">
      <div className="referral-link__url" title={link}>
        {link}
      </div>
      <div className="referral-link__actions">
        <button className="btn btn--primary" onClick={() => void handleCopy()}>
          {copied ? "Copied" : "Copy"}
        </button>
        <button className="btn" onClick={() => setShowQr((on) => !on)}>
          {showQr ? "Hide QR" : "QR"}
        </button>
        <button className="btn" onClick={() => void handleOpen()}>
          Open
        </button>
      </div>
      {showQr && (
        <div className="referral-link__qr">
          <QrCode value={link} />
        </div>
      )}
      {error && <div className="settings-panel__hint">{error}</div>}
    </div>
  );
}
