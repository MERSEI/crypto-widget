import { useEffect, useState } from "react";
import { useWalletStore } from "../../core/store/wallet";
import { HistoryList } from "./HistoryList";
import { Portfolio } from "./Portfolio";
import { SendForm } from "./SendForm";
import { WalletSettingsForm } from "./WalletSettingsForm";
import { WalletSetup } from "./WalletSetup";

type Tab = "portfolio" | "send" | "history" | "settings";

const TABS: Array<{ id: Tab; label: string }> = [
  { id: "portfolio", label: "PORTFOLIO" },
  { id: "send", label: "SEND" },
  { id: "history", label: "HISTORY" },
  { id: "settings", label: "SETTINGS" },
];

/** `0x1234…cdef`, with the full address on hover. Nothing is ever truncated on the send path —
 *  only here, where the address is being read rather than used. */
function shortAddress(address: string): string {
  return `${address.slice(0, 6)}…${address.slice(-4)}`;
}

/**
 * The wallet window — a normal, decorated, resizable window, unlike the pill.
 *
 * A separate top-level root rather than a route inside `App` for the same reason the futures
 * terminal is: the pill window is transparent, undecorated and 140×30, which suits neither a
 * balance table nor an address someone has to read character by character before sending funds
 * to it.
 */
export function WalletWindow() {
  const state = useWalletStore((s) => s.state);
  const hydrate = useWalletStore((s) => s.hydrate);
  const [tab, setTab] = useState<Tab>("portfolio");
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    void hydrate();
  }, [hydrate]);

  if (!state) {
    return (
      <div className="wallet-window">
        <div className="onboarding">
          <span>Loading…</span>
        </div>
      </div>
    );
  }

  const address = state.status.address;

  async function copyAddress() {
    if (!address) return;
    await navigator.clipboard.writeText(address);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  }

  return (
    <div className="wallet-window">
      <header className="wallet-window__header">
        <span className="panel__title">WALLET</span>
        {address ? (
          <button
            className="wallet-window__address mono-nums"
            title={`${address} — click to copy`}
            onClick={() => void copyAddress()}
          >
            {copied ? "copied" : shortAddress(address)}
          </button>
        ) : (
          <span className="wallet-window__address">not set up</span>
        )}
        <div className="futures__toolbar-spacer" />
        <span className="wallet-window__chain mono-nums" title={state.settings.rpcUrl}>
          chain {state.settings.chainId}
        </span>
        <span className="wallet-window__account mono-nums">
          account {state.settings.accountIndex}
        </span>
      </header>

      {!state.status.initialized ? (
        <WalletSetup />
      ) : (
        <>
          <div className="panel__tabs">
            {TABS.map((entry) => (
              <button
                key={entry.id}
                className={`panel-tab ${tab === entry.id ? "panel-tab--active" : ""}`}
                onClick={() => setTab(entry.id)}
              >
                {entry.label}
              </button>
            ))}
          </div>

          <div className="wallet-window__body">
            {tab === "portfolio" && <Portfolio />}
            {tab === "send" && <SendForm />}
            {tab === "history" && <HistoryList />}
            {tab === "settings" && <WalletSettingsForm />}
          </div>
        </>
      )}
    </div>
  );
}
