pub mod mxb;
pub mod mxbshop;

use serde::Serialize;

/// A mod as it appears in a search/browse listing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModSummary {
    pub id: u64,
    pub slug: String,
    pub title: String,
    pub link: String,
    pub date: String,
    pub image: Option<String>,
    pub category_id: u32,
}

/// One download choice on a mod page. Hosts vary (Google Drive, MediaFire, …).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadOption {
    pub url: String,
    pub host: String,
    /// The author's recommended file ("Default" flag on the page).
    pub is_default: bool,
    /// A dedicated-server build — not needed for normal play.
    pub is_server: bool,
    pub label: String,
}

/// Full detail for a single mod page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModDetail {
    pub id: u64,
    pub slug: String,
    pub title: String,
    pub link: String,
    pub date: String,
    pub description_html: String,
    pub images: Vec<String>,
    pub version: Option<String>,
    pub downloads: Vec<DownloadOption>,
}

/// A swappable catalog backend. Today only [`mxb::MxbModsSource`] implements it;
/// a future API-backed source can implement the same trait.
#[allow(async_fn_in_trait)]
pub trait ModSource {
    async fn search(
        &self,
        query: &str,
        category_id: u32,
        page: u32,
    ) -> anyhow::Result<Vec<ModSummary>>;

    async fn detail(&self, slug: &str) -> anyhow::Result<ModDetail>;
}
