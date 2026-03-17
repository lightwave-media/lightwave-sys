//! macOS Spotlight (mdfind) search integration.
//!
//! Query indexed metadata across the filesystem using Spotlight.

use anyhow::Result;
use serde::Serialize;

/// A Spotlight search result.
#[derive(Debug, Clone, Serialize)]
pub struct SpotlightResult {
    pub path: String,
    pub kind: Option<String>,
    pub name: Option<String>,
}

/// Search Spotlight for files matching a query.
///
/// Uses `mdfind` under the hood for reliable, indexed search.
pub fn search(query: &str, limit: usize) -> Result<Vec<SpotlightResult>> {
    search_in_directory(query, None, limit)
}

/// Search Spotlight within a specific directory.
pub fn search_in_directory(
    query: &str,
    directory: Option<&str>,
    limit: usize,
) -> Result<Vec<SpotlightResult>> {
    let mut cmd = std::process::Command::new("mdfind");

    if let Some(dir) = directory {
        cmd.args(["-onlyin", dir]);
    }

    cmd.arg(query);

    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    let results: Vec<SpotlightResult> = stdout
        .lines()
        .take(limit)
        .map(|path| {
            let name = std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string());
            let kind = std::path::Path::new(path)
                .extension()
                .map(|e| e.to_string_lossy().to_string());
            SpotlightResult {
                path: path.to_string(),
                kind,
                name,
            }
        })
        .collect();

    Ok(results)
}

/// Search Spotlight by file kind (e.g., "public.image", "com.adobe.pdf").
pub fn search_by_kind(kind: &str, limit: usize) -> Result<Vec<SpotlightResult>> {
    let query = format!("kMDItemContentType == '{kind}'");
    search(&query, limit)
}

/// Search for files modified within the last N seconds.
pub fn search_recently_modified(
    seconds: u64,
    directory: Option<&str>,
) -> Result<Vec<SpotlightResult>> {
    let query = format!("kMDItemFSContentChangeDate >= $time.now(-{seconds})");
    search_in_directory(&query, directory, 100)
}
