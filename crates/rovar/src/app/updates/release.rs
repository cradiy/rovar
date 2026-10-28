use anyhow::{Context as _, ensure};
use semver::Version;
use std::time::Duration;

const API: &str = "https://api.github.com/repos/cradiy/rovar/releases/latest";
pub(super) const RELEASES: &str = "https://github.com/cradiy/rovar/releases";

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Release {
    pub version: String,
    pub url: String,
}

#[derive(serde::Deserialize)]
struct Published {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}

fn newer(bytes: &[u8], current: &str) -> anyhow::Result<Option<Release>> {
    let published: Published = serde_json::from_slice(bytes)?;
    if published.draft || published.prerelease {
        return Ok(None);
    }
    let version = Version::parse(
        published
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&published.tag_name),
    )
    .context("Release tag is not a semantic version")?;
    if !version.pre.is_empty() || !version.cmp_precedence(&Version::parse(current)?).is_gt() {
        return Ok(None);
    }
    let mut url = reqwest::Url::parse(RELEASES)?;
    url.path_segments_mut()
        .unwrap()
        .push("tag")
        .push(&published.tag_name);
    Ok(Some(Release {
        version: version.to_string(),
        url: url.into(),
    }))
}

/// Runs on the background executor, with no automatic retry or polling.
pub(super) fn check() -> anyhow::Result<Option<Release>> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let client = reqwest::Client::builder()
                .user_agent(concat!("Rovar/", env!("CARGO_PKG_VERSION")))
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(15))
                .build()?;
            let response = client
                .get(API)
                .header("Accept", "application/vnd.github+json")
                .send()
                .await?;
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                return Ok(None);
            }
            let mut response = response.error_for_status()?;
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                ensure!(
                    bytes.len() + chunk.len() <= 1024 * 1024,
                    "Release response is too large"
                );
                bytes.extend_from_slice(&chunk);
            }
            newer(&bytes, env!("CARGO_PKG_VERSION"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_release_comparison_uses_precedence_and_links_to_the_exact_tag() {
        let release = |tag, draft, prerelease| {
            serde_json::to_vec(&serde_json::json!({
                "tag_name":tag,"draft":draft,"prerelease":prerelease,
                "html_url":"https://unrelated.example/download"
            }))
            .unwrap()
        };
        let next = newer(&release("v0.10.0", false, false), "0.9.9")
            .unwrap()
            .unwrap();
        assert_eq!(
            next.url,
            "https://github.com/cradiy/rovar/releases/tag/v0.10.0"
        );
        for (tag, draft, prerelease) in [
            ("v0.9.8", false, false),
            ("0.9.9+rebuilt", false, false),
            ("v0.10.0-beta.1", false, false),
            ("v1.0.0", true, false),
            ("v1.0.0", false, true),
        ] {
            assert!(
                newer(&release(tag, draft, prerelease), "0.9.9")
                    .unwrap()
                    .is_none()
            );
        }
        assert!(
            newer(&release("v1.0.0", false, false), "1.0.0-beta.2")
                .unwrap()
                .is_some()
        );
        assert!(newer(&release("invalid-tag", false, false), "0.9.9").is_err());
    }
}
