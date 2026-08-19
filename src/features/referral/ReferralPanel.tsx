import { useEffect, useState } from "react";
import { useReferralStore } from "../../core/store/referral";
import { ReferralLink } from "./ReferralLink";

/**
 * Referral tab: pick a partner, store the affiliate ID, get the link.
 *
 * The template field is deliberately visible rather than tucked away in settings. Affiliate
 * URL schemes are published inside partner dashboards and they change; a link built from a
 * stale default still opens the exchange and simply never attributes the signup. Showing the
 * format — and flagging it as unverified until the user confirms it — is what makes that
 * failure visible before it costs anything.
 */
export function ReferralPanel() {
  const partners = useReferralStore((s) => s.partners);
  const profile = useReferralStore((s) => s.profile);
  const loaded = useReferralStore((s) => s.loaded);
  const load = useReferralStore((s) => s.load);
  const selectPartner = useReferralStore((s) => s.selectPartner);
  const setAffiliateId = useReferralStore((s) => s.setAffiliateId);
  const setTemplate = useReferralStore((s) => s.setTemplate);
  const openUrl = useReferralStore((s) => s.openUrl);

  const [idDraft, setIdDraft] = useState("");
  const [templateDraft, setTemplateDraft] = useState("");
  const [showTemplate, setShowTemplate] = useState(false);

  useEffect(() => {
    if (!loaded) void load();
  }, [loaded, load]);

  // The drafts follow the backend's profile: switching partners has to swap both fields to
  // that partner's stored values, not carry the previous one's text over.
  useEffect(() => {
    setIdDraft(profile?.affiliateId ?? "");
    setTemplateDraft(profile?.template ?? "");
  }, [profile?.partner, profile?.affiliateId, profile?.template]);

  if (!loaded || !profile) {
    return (
      <div className="panel__body">
        <div className="onboarding">
          <span>Loading…</span>
        </div>
      </div>
    );
  }

  const partner = partners.find((p) => p.id === profile.partner) ?? null;

  return (
    <div className="panel__body referral-panel">
      <div className="settings-panel__row">
        <span className="settings-panel__label">Partner</span>
        <select
          value={profile.partner ?? ""}
          onChange={(e) => void selectPartner(e.target.value)}
        >
          <option value="" disabled>
            Choose…
          </option>
          {partners.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </div>

      {!partner && (
        <div className="settings-panel__hint">
          Pick the program you are signed up to. The widget stores your affiliate ID and builds
          the link — the partner does the tracking and the payouts.
        </div>
      )}

      {partner && (
        <>
          {partner.commission && (
            <div className="settings-panel__hint">{partner.commission}</div>
          )}

          <div className="referral-panel__field">
            <label htmlFor="referral-id">Affiliate ID</label>
            <input
              id="referral-id"
              value={idDraft}
              placeholder="from your partner dashboard"
              spellCheck={false}
              onChange={(e) => setIdDraft(e.target.value)}
              onBlur={() => void setAffiliateId(idDraft)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void setAffiliateId(idDraft);
              }}
            />
          </div>

          <button className="referral-panel__disclosure" onClick={() => setShowTemplate((on) => !on)}>
            {showTemplate ? "▾" : "▸"} Link format
            {profile.templateIsDefault && " (unverified default)"}
          </button>

          {showTemplate && (
            <div className="referral-panel__field">
              <input
                value={templateDraft}
                placeholder="https://partner.example/?ref={id}"
                spellCheck={false}
                onChange={(e) => setTemplateDraft(e.target.value)}
                onBlur={() => void setTemplate(templateDraft)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void setTemplate(templateDraft);
                }}
              />
              <div className="settings-panel__hint">
                Check this against your dashboard — a wrong parameter still opens the site but
                credits no one. Use <code>{"{id}"}</code> where your ID goes; clear the field to
                restore the default.
              </div>
            </div>
          )}

          {profile.link && <ReferralLink link={profile.link} />}
          {profile.error && <div className="referral-panel__error">{profile.error}</div>}

          {partner.dashboardUrl && (
            <button
              className="referral-panel__dashboard"
              onClick={() => void openUrl(partner.dashboardUrl)}
            >
              Open {partner.name} dashboard ↗
            </button>
          )}

          <div className="settings-panel__hint">
            Clicks, volume and earnings are counted by {partner.name} — open the dashboard to see
            them.
          </div>
        </>
      )}
    </div>
  );
}
