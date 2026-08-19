use super::links::{self, Partner};
use super::ReferralSettings;
use crate::AppState;
use serde::Serialize;
use tauri::{AppHandle, State};

/// Everything the referral panel needs in one round-trip: the stored inputs, the link they
/// currently produce, and — when they don't produce one — why.
///
/// The link is built in Rust rather than in the renderer so there is exactly one implementation
/// of the format rules, and it is the one covered by tests.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferralProfile {
    pub partner: Option<String>,
    pub affiliate_id: String,
    /// The override in force, or the partner's built-in default, or `""` when neither exists.
    pub template: String,
    /// True when `template` is the built-in suggestion rather than something the user entered —
    /// the panel labels it as unverified, because a default that looks right and pays nothing
    /// is the failure mode this feature has to avoid.
    pub template_is_default: bool,
    pub link: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn get_referral_partners() -> Result<Vec<Partner>, String> {
    Ok(links::PARTNERS.to_vec())
}

fn profile_from(settings: &ReferralSettings) -> ReferralProfile {
    let partner_id = settings.partner.clone();
    let affiliate_id = settings.active_id().unwrap_or_default().to_string();

    let override_template = settings.active_template();
    let default_template = partner_id
        .as_deref()
        .and_then(links::find)
        .and_then(|p| p.default_template);

    let (template, template_is_default) = match (override_template, default_template) {
        (Some(t), _) => (t.to_string(), false),
        (None, Some(t)) => (t.to_string(), true),
        (None, None) => (String::new(), false),
    };

    let (link, error) = match settings.active_link() {
        Some(Ok(link)) => (Some(link), None),
        Some(Err(e)) => (None, Some(e)),
        None => (None, None),
    };

    ReferralProfile {
        partner: partner_id,
        affiliate_id,
        template,
        template_is_default,
        link,
        error,
    }
}

#[tauri::command]
pub async fn get_referral_profile(state: State<'_, AppState>) -> Result<ReferralProfile, String> {
    Ok(profile_from(&state.config.snapshot().referral))
}

#[tauri::command]
pub async fn set_referral_partner(
    state: State<'_, AppState>,
    partner: Option<String>,
) -> Result<ReferralProfile, String> {
    if let Some(id) = partner.as_deref() {
        if links::find(id).is_none() {
            return Err(format!("unknown partner: {id}"));
        }
    }
    {
        let mut settings = state.config.settings.write().unwrap();
        settings.referral.partner = partner;
    }
    state.config.mark_dirty();
    Ok(profile_from(&state.config.snapshot().referral))
}

/// Stores the affiliate ID for a partner and returns the resulting profile, so the panel shows
/// the built link — or the reason there isn't one — without a second round-trip.
#[tauri::command]
pub async fn set_referral_id(
    state: State<'_, AppState>,
    partner: String,
    id: String,
) -> Result<ReferralProfile, String> {
    if links::find(&partner).is_none() {
        return Err(format!("unknown partner: {partner}"));
    }
    {
        let mut settings = state.config.settings.write().unwrap();
        let id = id.trim().to_string();
        if id.is_empty() {
            settings.referral.ids.remove(&partner);
        } else {
            settings.referral.ids.insert(partner, id);
        }
    }
    state.config.mark_dirty();
    Ok(profile_from(&state.config.snapshot().referral))
}

/// Overrides the link format for a partner. An empty string clears the override and restores
/// the built-in default.
#[tauri::command]
pub async fn set_referral_template(
    state: State<'_, AppState>,
    partner: String,
    template: String,
) -> Result<ReferralProfile, String> {
    if links::find(&partner).is_none() {
        return Err(format!("unknown partner: {partner}"));
    }
    let template = template.trim().to_string();
    // Reject a broken template at the point of entry rather than storing it and failing at
    // every later render.
    if !template.is_empty() && !template.contains(links::ID_PLACEHOLDER) {
        return Err(format!(
            "the template must contain {} where the ID goes",
            links::ID_PLACEHOLDER
        ));
    }
    {
        let mut settings = state.config.settings.write().unwrap();
        if template.is_empty() {
            settings.referral.templates.remove(&partner);
        } else {
            settings.referral.templates.insert(partner, template);
        }
    }
    state.config.mark_dirty();
    Ok(profile_from(&state.config.snapshot().referral))
}

/// Opens a referral-related URL in the system browser.
///
/// The renderer passes what it wants opened, so this validates before handing anything to the
/// OS: https only, and only a URL the app itself would have produced — the current referral
/// link or a known partner's dashboard. Without that check this command would be a
/// general-purpose "open anything" primitive reachable from the webview.
#[tauri::command]
pub async fn open_referral_url(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
) -> Result<(), String> {
    links::validate_url(&url)?;

    let referral = state.config.snapshot().referral;
    let is_own_link = matches!(referral.active_link(), Some(Ok(link)) if link == url);
    let is_partner_dashboard = links::PARTNERS
        .iter()
        .any(|p| !p.dashboard_url.is_empty() && p.dashboard_url == url);
    if !is_own_link && !is_partner_dashboard {
        return Err("refusing to open a URL the app did not generate".into());
    }

    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("could not open the browser: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_with(partner: &str, id: &str) -> ReferralSettings {
        let mut settings = ReferralSettings {
            partner: Some(partner.into()),
            ..Default::default()
        };
        settings.ids.insert(partner.into(), id.into());
        settings
    }

    #[test]
    fn a_complete_profile_carries_the_link_and_no_error() {
        let profile = profile_from(&settings_with("binance", "AB12CD"));
        assert_eq!(
            profile.link.as_deref(),
            Some("https://accounts.binance.com/register?ref=AB12CD")
        );
        assert!(profile.error.is_none());
        assert!(profile.template_is_default, "nothing was overridden");
    }

    #[test]
    fn an_untouched_profile_has_neither_link_nor_error() {
        let profile = profile_from(&ReferralSettings::default());
        assert!(profile.link.is_none());
        assert!(profile.error.is_none(), "not having started is not an error");
        assert!(profile.template.is_empty());
    }

    #[test]
    fn a_partner_with_no_default_reports_what_is_missing() {
        let profile = profile_from(&settings_with("quickex", "abc"));
        assert!(profile.link.is_none());
        assert!(profile.error.unwrap().contains("quickex.io/affiliate"));
        assert!(!profile.template_is_default);
    }

    #[test]
    fn an_override_is_reported_as_user_supplied() {
        let mut settings = settings_with("changenow", "xyz");
        settings
            .templates
            .insert("changenow".into(), "https://changenow.io/exchange?link_id={id}".into());
        let profile = profile_from(&settings);
        assert!(!profile.template_is_default);
        assert_eq!(
            profile.link.as_deref(),
            Some("https://changenow.io/exchange?link_id=xyz")
        );
    }

    #[test]
    fn a_bad_id_surfaces_as_an_error_not_a_silent_missing_link() {
        let profile = profile_from(&settings_with("binance", "https://binance.com/?ref=x"));
        assert!(profile.link.is_none());
        assert!(profile.error.is_some());
    }
}
