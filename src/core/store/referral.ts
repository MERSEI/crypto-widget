import { create } from "zustand";
import { commands } from "../ipc/commands";
import type { Partner, ReferralProfile } from "../../types/referral";

/**
 * Referral state.
 *
 * Rust builds the link and owns the validation rules, so this store never derives one — it
 * stores whatever the backend answered. Every mutating command returns the rebuilt profile,
 * which is why there is no separate refresh after a write.
 */
interface ReferralState {
  partners: Partner[];
  profile: ReferralProfile | null;
  loaded: boolean;
  load: () => Promise<void>;
  selectPartner: (partner: string) => Promise<void>;
  setAffiliateId: (id: string) => Promise<void>;
  setTemplate: (template: string) => Promise<void>;
  openUrl: (url: string) => Promise<void>;
}

const EMPTY_PROFILE: ReferralProfile = {
  partner: null,
  affiliateId: "",
  template: "",
  templateIsDefault: false,
  link: null,
  error: null,
};

export const useReferralStore = create<ReferralState>((set, get) => ({
  partners: [],
  profile: null,
  loaded: false,

  load: async () => {
    const [partners, profile] = await Promise.all([
      commands.getReferralPartners(),
      commands.getReferralProfile(),
    ]);
    set({ partners, profile, loaded: true });
  },

  selectPartner: async (partner) => {
    set({ profile: await commands.setReferralPartner(partner) });
  },

  setAffiliateId: async (id) => {
    const partner = get().profile?.partner;
    if (!partner) return;
    set({ profile: await commands.setReferralId(partner, id) });
  },

  setTemplate: async (template) => {
    const partner = get().profile?.partner;
    if (!partner) return;
    try {
      set({ profile: await commands.setReferralTemplate(partner, template) });
    } catch (e) {
      // A rejected template (no `{id}` placeholder) is a message for the user, not a crash:
      // keep the previous profile and surface the reason where the link would have been.
      set((state) => ({ profile: { ...(state.profile ?? EMPTY_PROFILE), link: null, error: String(e) } }));
    }
  },

  openUrl: async (url) => {
    await commands.openReferralUrl(url);
  },
}));
