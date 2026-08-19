/** Mirrors `Partner` in `src-tauri/src/referral/links.rs`. */
export interface Partner {
  id: string;
  name: string;
  /** Suggested link format, or null where the partner does not publish one. */
  defaultTemplate: string | null;
  dashboardUrl: string;
  commission: string;
}

/** Mirrors `ReferralProfile` in `src-tauri/src/referral/commands.rs`.
 *
 *  `link` and `error` are mutually exclusive, and both are null before the user has entered
 *  anything — "not started" is not a failure. */
export interface ReferralProfile {
  partner: string | null;
  affiliateId: string;
  template: string;
  /** True when `template` is the built-in suggestion rather than one the user pasted in. */
  templateIsDefault: boolean;
  link: string | null;
  error: string | null;
}
