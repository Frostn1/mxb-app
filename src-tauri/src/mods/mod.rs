pub mod mxb;
pub mod mxbshop;

use serde::{Deserialize, Serialize};

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

/// The order a browse listing comes back in.
///
/// Deliberately short, because the catalog gives us little to work with: mxb-mods.com
/// accepts `orderby` on its REST API and then ignores it — `date`, `title`, `modified`
/// and `relevance` all return the same fixed listing. So alphabetical and
/// recently-updated orders are simply not on offer. What does work is `offset` (which
/// buys us oldest-first by counting back from the end) and the site's popular-posts
/// plugin, which ranks a category by view count over a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModSort {
    /// The catalog's own order — newest first, near enough.
    #[default]
    Newest,
    Oldest,
    /// Most viewed, all time.
    PopularAll,
    PopularMonth,
    PopularWeek,
}

impl ModSort {
    /// The popular-posts `range` this maps to, or `None` for the plain catalog listing.
    pub fn popular_range(self) -> Option<&'static str> {
        match self {
            ModSort::PopularAll => Some("all"),
            ModSort::PopularMonth => Some("last30days"),
            ModSort::PopularWeek => Some("last7days"),
            ModSort::Newest | ModSort::Oldest => None,
        }
    }
}

/// A mod's community score on mxb-mods.com — the same average and vote count the
/// site prints under each listing thumbnail.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModRating {
    /// Mean score out of 5. Meaningless when `count` is 0.
    pub average: f32,
    pub count: u32,
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
    /// The post's category names, verbatim ("2023 KTM 450 SX-F OEM", "Liveries", "KTM").
    /// A livery's model category names the bike it's for far more precisely than its title
    /// does, which is what the install picker ranks destinations by.
    pub categories: Vec<String>,
}

#[allow(async_fn_in_trait)]
pub trait ModSource {
    async fn search(
        &self,
        query: &str,
        category_id: u32,
        page: u32,
        sort: ModSort,
    ) -> anyhow::Result<Vec<ModSummary>>;

    async fn detail(&self, slug: &str) -> anyhow::Result<ModDetail>;
}
