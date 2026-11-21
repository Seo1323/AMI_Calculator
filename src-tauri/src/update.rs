//! "Check for updates": asks the public releases repository (the main repository is private, so
//! GitHub's API would answer 404 without a login) and opens its download page in the browser.
//!
//! shortcut: a manual check against GitHub's unauthenticated API (60 requests an hour per address)
//! that opens a page, not an installer; upgrade to a signed in-app updater once releases are signed
//! and published in one place.

use serde::{Deserialize, Serialize};

const REPO: &str = "Seo1323/AMI_Calculator-releases";
/// Only pages below this prefix may be opened or reported.
const PAGE_PREFIX: &str = "https://github.com/Seo1323/AMI_Calculator-releases/releases/";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub newer: bool,
    pub url: String,
}

/// "v1.2.3" or "1.2.3" (nothing else: a pre-release suffix is not a version we offer).
fn version(tag: &str) -> Option<(u64, u64, u64)> {
    let mut parts = tag.trim_start_matches('v').split('.');
    let v = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(v)
}

pub fn page_allowed(url: &str) -> bool {
    url.starts_with(PAGE_PREFIX) && !url.contains(|c: char| c.is_whitespace() || c == '\\')
}

/// The highest published version in GitHub's release list, compared with `current`.
fn newest(releases_json: &str, current: &str) -> Result<Option<UpdateInfo>, String> {
    let releases: Vec<Release> = serde_json::from_str(releases_json).map_err(|e| e.to_string())?;
    let best = releases
        .into_iter()
        .filter(|r| !r.draft && page_allowed(&r.html_url))
        .filter_map(|r| version(&r.tag_name).map(|v| (v, r)))
        .max_by_key(|(v, _)| *v);
    Ok(best.map(|(v, r)| UpdateInfo {
        current: current.to_string(),
        latest: r.tag_name.trim_start_matches('v').to_string(),
        newer: version(current).is_some_and(|c| v > c),
        url: r.html_url,
    }))
}

pub async fn check() -> Result<Option<UpdateInfo>, String> {
    let client = crate::fetch::client().map_err(|e| e.without_url().to_string())?;
    let url = format!("https://api.github.com/repos/{REPO}/releases?per_page=20");
    let body = crate::fetch::get(&client, &url).await?;
    newest(&body, env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, draft: bool) -> String {
        format!(r#"{{"tag_name":"{tag}","html_url":"{PAGE_PREFIX}tag/{tag}","draft":{draft}}}"#)
    }

    #[test]
    fn the_highest_published_version_wins_not_the_first_listed() {
        let list = format!(
            "[{},{},{}]",
            release("v0.4.9", false),
            release("v0.10.0", false),
            release("v0.6.1", false)
        );
        let u = newest(&list, "0.6.0").unwrap().unwrap();
        assert_eq!(
            (u.latest.as_str(), u.newer),
            ("0.10.0", true),
            "0.10.0 is above 0.6.1"
        );
    }

    #[test]
    fn nothing_newer_means_newer_is_false() {
        let list = format!("[{}]", release("v0.6.0", false));
        assert!(!newest(&list, "0.6.0").unwrap().unwrap().newer);
        assert!(!newest(&list, "0.7.0").unwrap().unwrap().newer);
    }

    #[test]
    fn drafts_odd_tags_and_foreign_pages_are_ignored() {
        let foreign = r#"{"tag_name":"v9.9.9","html_url":"https://evil.example/releases/tag/v9.9.9","draft":false}"#;
        let list = format!(
            "[{},{},{},{}]",
            release("v1.0.0", true),
            release("v1.0.0-beta", false),
            release("nightly", false),
            foreign
        );
        assert_eq!(newest(&list, "0.6.0").unwrap(), None);
        assert!(newest("not json", "0.6.0").is_err());
    }

    #[test]
    fn only_pages_of_the_releases_repository_may_be_opened() {
        assert!(page_allowed(&format!("{PAGE_PREFIX}tag/v0.6.1")));
        for bad in [
            "https://github.com/Seo1323/other/releases/tag/v1",
            "http://github.com/Seo1323/AMI_Calculator-releases/releases/tag/v1",
            "javascript:alert(1)",
            "file:///etc/passwd",
            &format!("{PAGE_PREFIX}tag/v1 --flag"),
            &format!("{PAGE_PREFIX}a\\b"),
            "",
        ] {
            assert!(!page_allowed(bad), "{bad}");
        }
    }
}
