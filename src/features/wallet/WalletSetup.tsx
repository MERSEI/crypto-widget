import { useState } from "react";
import { commands } from "../../core/ipc/commands";
import { useWalletStore, walletErrorMessage } from "../../core/store/wallet";
import { SeedPhrase } from "./SeedPhrase";

type Mode = "choose" | "created" | "import";

/**
 * First run: create a wallet or restore one.
 *
 * The created phrase is shown once, here, and the "continue" button stays disabled until the
 * user ticks that they wrote it down. That checkbox is not ceremony — this is the only backup
 * that exists, and a wallet funded before it is written down is one disk failure from gone. It
 * can still be read later from Settings → Reveal, which is why nothing here blocks the user
 * permanently.
 */
export function WalletSetup() {
  const apply = useWalletStore((s) => s.apply);
  const hydrate = useWalletStore((s) => s.hydrate);

  const [mode, setMode] = useState<Mode>("choose");
  const [phrase, setPhrase] = useState("");
  const [typed, setTyped] = useState("");
  const [written, setWritten] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function create() {
    setBusy(true);
    setError(null);
    try {
      const created = await commands.createWallet();
      setPhrase(created.phrase);
      apply(created.state);
      setMode("created");
    } catch (e) {
      setError(walletErrorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function importPhrase() {
    setBusy(true);
    setError(null);
    try {
      apply(await commands.importWallet(typed));
      setTyped("");
      await hydrate();
    } catch (e) {
      setError(walletErrorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="wallet-setup">
      {error && <div className="wallet__error">{error}</div>}

      {mode === "choose" && (
        <>
          <h2 className="wallet-setup__title">No wallet on this machine yet</h2>
          <p className="wallet-setup__lead">
            Keys are derived from a 12-word phrase and stored in the Windows Credential Manager.
            They never reach the interface, and nothing is sent anywhere until you sign a
            transfer.
          </p>
          <div className="wallet-setup__actions">
            <button className="btn btn--primary" disabled={busy} onClick={() => void create()}>
              Create a new wallet
            </button>
            <button className="btn" disabled={busy} onClick={() => setMode("import")}>
              I have a seed phrase
            </button>
          </div>
        </>
      )}

      {mode === "created" && (
        <>
          <h2 className="wallet-setup__title">Write these 12 words down</h2>
          <p className="wallet-setup__lead">
            In this order, on paper. Anyone holding them holds the funds; losing them loses the
            funds. There is no reset link.
          </p>
          <SeedPhrase phrase={phrase} />
          <label className="wallet-setup__confirm">
            <input
              type="checkbox"
              checked={written}
              onChange={(e) => setWritten(e.target.checked)}
            />
            I wrote the phrase down somewhere safe
          </label>
          <div className="wallet-setup__actions">
            <button
              className="btn btn--primary"
              disabled={!written}
              onClick={() => {
                // Dropped from state as soon as it is acknowledged, so a phrase does not sit in
                // renderer memory for the rest of the session.
                setPhrase("");
                void hydrate();
              }}
            >
              Open the wallet
            </button>
          </div>
        </>
      )}

      {mode === "import" && (
        <>
          <h2 className="wallet-setup__title">Restore from a seed phrase</h2>
          <p className="wallet-setup__lead">
            12, 15, 18, 21 or 24 BIP-39 words, separated by spaces. The checksum is verified
            before anything is stored.
          </p>
          <textarea
            className="wallet-setup__input"
            rows={3}
            spellCheck={false}
            autoComplete="off"
            placeholder="witch collapse practice feed shame open despair creek road again ice least"
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
          />
          <div className="wallet-setup__actions">
            <button
              className="btn btn--primary"
              disabled={busy || typed.trim().length === 0}
              onClick={() => void importPhrase()}
            >
              Restore
            </button>
            <button className="btn" disabled={busy} onClick={() => setMode("choose")}>
              Back
            </button>
          </div>
        </>
      )}
    </div>
  );
}
