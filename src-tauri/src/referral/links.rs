//! Partner catalog and referral-link construction.
//!
//! The link format is **editable**, and that is the whole design. Exchanges publish their
//! affiliate URL scheme only inside the partner dashboard, and they change it: a link with the
//! wrong parameter name still opens the exchange, still converts, and simply never attributes
//! the signup. The failure is silent and shows up as "why did I earn nothing" three months
//! later. So the built-in templates here are *defaults to check against the dashboard*, not
//! facts — and any of them can be overridden with the exact string the partner handed out.
//!
//! An id is validated rather than escaped. Affiliate ids are alphanumeric handles; anything
//! with a `?`, `&`, or `/` in it is a user pasting the wrong thing (usually a whole URL), and
//! silently percent-encoding it would produce a link that looks right and pays nobody.

use serde::Serialize;

/// Placeholder replaced by the affiliate id when a template is rendered.
pub const ID_PLACEHOLDER: &str = "{id}";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Partner {
    pub id: &'static str,
    pub name: &'static str,
    /// Suggested link format. `None` where the scheme is not publicly documented and has to be
    /// copied out of the partner dashboard.
    pub default_template: Option<&'static str>,
    /// Where the user finds their id, their real link, and their payout numbers.
    pub dashboard_url: &'static str,
    /// Human-readable commission summary, shown in the panel. Indicative — partners change
    /// their rates, and the dashboard is the authority.
    pub commission: &'static str,
}

/// Partners the panel offers. `custom` is the escape hatch: paste any link, from any program.
pub const PARTNERS: &[Partner] = &[
    Partner {
        id: "binance",
        name: "Binance",
        default_template: Some("https://accounts.binance.com/register?ref={id}"),
        dashboard_url: "https://www.binance.com/en/activity/referral",
        commission: "up to 40% of referred trading fees",
    },
    Partner {
        id: "bybit",
        name: "Bybit",
        default_template: Some("https://www.bybit.com/invite?ref={id}"),
        dashboard_url: "https://www.bybit.com/en/promo/affiliate-program",
        commission: "up to 30% of referred trading fees",
    },
    Partner {
        id: "changenow",
        name: "ChangeNOW",
        default_template: Some("https://changenow.io/?link_id={id}"),
        dashboard_url: "https://changenow.io/affiliate",
        commission: "up to 0.4% of each swap",
    },
    Partner {
        id: "quickex",
        name: "Quickex",
        default_template: None,
        dashboard_url: "https://quickex.io/affiliate",
        commission: "20% of the exchange margin",
    },
    Partner {
        id: "custom",
        name: "Custom link",
        default_template: None,
        dashboard_url: "",
        commission: "",
    },
];

pub fn find(partner_id: &str) -> Option<&'static Partner> {
    PARTNERS.iter().find(|p| p.id == partner_id)
}

/// Rejects anything that is not a plain affiliate handle.
///
/// The common mistake is pasting the whole link into the id field. Catching it here turns a
/// silently broken link into a message the user can act on.
fn validate_id(id: &str) -> Result<&str, String> {
    let id = id.trim();
    if id.is_empty() {
        return Err("enter your affiliate ID first".into());
    }
    if id.contains("://") || id.contains('?') || id.contains('&') || id.contains('/') {
        return Err("that looks like a full link — paste just the ID, or switch to Custom link".into());
    }
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) {
        return Err("an affiliate ID may only contain letters, digits, '-', '_' and '.'".into());
    }
    Ok(id)
}

/// Renders `template` with `id` substituted, then checks the result is a plausible https link.
pub fn render(template: &str, id: &str) -> Result<String, String> {
    let template = template.trim();
    if template.is_empty() {
        return Err("no link template set for this partner — paste the one from its dashboard".into());
    }
    if !template.contains(ID_PLACEHOLDER) {
        return Err(format!("the template must contain {ID_PLACEHOLDER} where the ID goes"));
    }
    let id = validate_id(id)?;
    let link = template.replace(ID_PLACEHOLDER, id);
    validate_url(&link)?;
    Ok(link)
}

/// Builds the link for a partner, preferring an explicit override over the built-in default.
pub fn build(partner_id: &str, id: &str, override_template: Option<&str>) -> Result<String, String> {
    let partner = find(partner_id).ok_or_else(|| format!("unknown partner: {partner_id}"))?;
    let template = override_template
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .or(partner.default_template)
        .ok_or_else(|| {
            format!(
                "{} does not publish its link format — copy your link from {} and paste it here",
                partner.name, partner.dashboard_url
            )
        })?;
    render(template, id)
}

/// A referral link that isn't https isn't a referral link: `http` would be downgraded or
/// blocked by the browser, and a stray `javascript:` string must never reach `opener`.
pub fn validate_url(link: &str) -> Result<(), String> {
    let parsed = url::Url::parse(link).map_err(|_| format!("not a valid URL: {link}"))?;
    if parsed.scheme() != "https" {
        return Err("a referral link must use https".into());
    }
    if !parsed.has_host() {
        return Err("that URL has no host".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_link_from_the_built_in_template() {
        let link = build("binance", "AB12CD34", None).unwrap();
        assert_eq!(link, "https://accounts.binance.com/register?ref=AB12CD34");
    }

    #[test]
    fn an_override_wins_over_the_default() {
        let link = build("binance", "AB12", Some("https://www.binance.com/en/register?ref={id}")).unwrap();
        assert_eq!(link, "https://www.binance.com/en/register?ref=AB12");
    }

    #[test]
    fn a_blank_override_falls_back_to_the_default() {
        let link = build("changenow", "xyz789", Some("   ")).unwrap();
        assert!(link.ends_with("link_id=xyz789"), "{link}");
    }

    #[test]
    fn a_partner_without_a_documented_format_demands_one() {
        let err = build("quickex", "abc", None).unwrap_err();
        assert!(err.contains("quickex.io/affiliate"), "{err}");

        // ...and accepts the link once it is supplied.
        let link = build("quickex", "abc", Some("https://quickex.io/?ref={id}")).unwrap();
        assert_eq!(link, "https://quickex.io/?ref=abc");
    }

    #[test]
    fn pasting_a_whole_url_into_the_id_field_is_caught() {
        let err = build("binance", "https://accounts.binance.com/register?ref=AB12", None).unwrap_err();
        assert!(err.contains("just the ID"), "{err}");
    }

    #[test]
    fn an_id_with_url_syntax_in_it_is_refused() {
        assert!(build("binance", "AB12&ref=someone-else", None).is_err());
        assert!(build("binance", "AB12/../evil", None).is_err());
        assert!(build("binance", "", None).is_err());
    }

    #[test]
    fn ids_that_partners_actually_issue_are_accepted() {
        assert!(build("binance", "REF_2026-08.x", None).is_ok());
    }

    #[test]
    fn a_template_without_the_placeholder_is_refused() {
        let err = render("https://changenow.io/?link_id=hardcoded", "abc").unwrap_err();
        assert!(err.contains(ID_PLACEHOLDER), "{err}");
    }

    #[test]
    fn non_https_and_script_urls_never_build() {
        assert!(render("http://changenow.io/?link_id={id}", "abc").is_err());
        assert!(render("javascript:alert({id})", "1").is_err());
        assert!(render("not a url {id}", "abc").is_err());
    }

    #[test]
    fn every_built_in_template_renders_and_is_valid() {
        for partner in PARTNERS {
            let Some(template) = partner.default_template else { continue };
            let link = render(template, "TESTID").unwrap_or_else(|e| panic!("{}: {e}", partner.id));
            assert!(link.contains("TESTID"), "{}: {link}", partner.id);
        }
    }

    #[test]
    fn the_catalog_has_no_duplicate_ids() {
        let mut ids: Vec<&str> = PARTNERS.iter().map(|p| p.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count);
    }
}
