use super::traits::{Tool, ToolResult};
use crate::security::SecurityPolicy;
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

/// Web search tool: searches the web via DuckDuckGo or Brave and returns results.
///
/// Uses DuckDuckGo HTML scraping (no API key) by default, or Brave Search API
/// (requires API key). Returns search results as structured text.
pub struct WebSearchTool {
    security: Arc<SecurityPolicy>,
    provider: String,
    brave_api_key: Option<String>,
    max_results: usize,
    timeout_secs: u64,
}

impl WebSearchTool {
    pub fn new(
        security: Arc<SecurityPolicy>,
        provider: String,
        brave_api_key: Option<String>,
        max_results: usize,
        timeout_secs: u64,
    ) -> Self {
        Self {
            security,
            provider,
            brave_api_key,
            max_results: max_results.clamp(1, 10),
            timeout_secs,
        }
    }

    async fn search_duckduckgo(&self, query: &str) -> anyhow::Result<Vec<SearchResult>> {
        let encoded_query = urlencoding::encode(query);
        let url = format!("https://html.duckduckgo.com/html/?q={encoded_query}");

        let timeout_secs = if self.timeout_secs == 0 {
            15
        } else {
            self.timeout_secs
        };

        let builder = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::limited(5))
            .user_agent("Augusta/0.1 (web_search)");
        let builder = crate::config::apply_runtime_proxy_to_builder(builder, "tool.web_search");
        let client = builder.build()?;

        let response = client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("DuckDuckGo returned HTTP {}", response.status().as_u16());
        }

        let body = response.text().await?;
        parse_duckduckgo_html(&body, self.max_results)
    }

    async fn search_brave(&self, query: &str) -> anyhow::Result<Vec<SearchResult>> {
        let api_key = self.brave_api_key.as_deref().ok_or_else(|| {
            anyhow::anyhow!("Brave Search requires brave_api_key in [web_search] config")
        })?;

        let encoded_query = urlencoding::encode(query);
        let url = format!(
            "https://api.search.brave.com/res/v1/web/search?q={encoded_query}&count={}",
            self.max_results
        );

        let timeout_secs = if self.timeout_secs == 0 {
            15
        } else {
            self.timeout_secs
        };

        let builder = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .connect_timeout(Duration::from_secs(10))
            .user_agent("Augusta/0.1 (web_search)");
        let builder = crate::config::apply_runtime_proxy_to_builder(builder, "tool.web_search");
        let client = builder.build()?;

        let response = client
            .get(&url)
            .header("X-Subscription-Token", api_key)
            .header("Accept", "application/json")
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("Brave Search returned HTTP {}", response.status().as_u16());
        }

        let body: serde_json::Value = response.json().await?;
        parse_brave_json(&body, self.max_results)
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web and return results with titles, URLs, and snippets. \
         Use for finding current information, documentation, or answers to questions. \
         Returns up to 5 results by default."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'query' parameter"))?;

        if query.trim().is_empty() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Search query cannot be empty".into()),
            });
        }

        if !self.security.can_act() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Action blocked: autonomy is read-only".into()),
            });
        }

        if !self.security.record_action() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Action blocked: rate limit exceeded".into()),
            });
        }

        let results = match self.provider.as_str() {
            "brave" => match self.search_brave(query).await {
                Ok(r) => r,
                Err(e) => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("Brave search failed: {e}")),
                    })
                }
            },
            _ => match self.search_duckduckgo(query).await {
                Ok(r) => r,
                Err(e) => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("DuckDuckGo search failed: {e}")),
                    })
                }
            },
        };

        if results.is_empty() {
            return Ok(ToolResult {
                success: true,
                output: "No results found.".into(),
                error: None,
            });
        }

        let output = format_results(&results);
        Ok(ToolResult {
            success: true,
            output,
            error: None,
        })
    }
}

// ── Search result types ──────────────────────────────────────────

struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

fn format_results(results: &[SearchResult]) -> String {
    let mut lines = Vec::new();
    for (i, result) in results.iter().enumerate() {
        lines.push(format!("{}. {}", i + 1, result.title));
        lines.push(format!("   URL: {}", result.url));
        if !result.snippet.is_empty() {
            lines.push(format!("   {}", result.snippet));
        }
        lines.push(String::new());
    }
    lines.join("\n").trim_end().to_string()
}

// ── DuckDuckGo HTML parsing ──────────────────────────────────────

fn parse_duckduckgo_html(html: &str, max_results: usize) -> anyhow::Result<Vec<SearchResult>> {
    let mut results = Vec::new();

    // DuckDuckGo HTML results are in <a class="result__a"> with <a class="result__snippet">
    // We use simple string parsing to avoid adding an HTML parser dependency.
    for result_block in html.split("class=\"result__body\"") {
        if results.len() >= max_results {
            break;
        }

        let title = extract_between(result_block, "class=\"result__a\"", "</a>");
        let snippet = extract_between(result_block, "class=\"result__snippet\"", "</a>");
        let url = extract_href(result_block, "class=\"result__url\"");

        if let Some(title) = title {
            let clean_title = strip_html_tags(title);
            let clean_snippet = snippet.map(strip_html_tags).unwrap_or_default();

            // DuckDuckGo wraps URLs in a redirect; extract the actual URL
            let clean_url = url.map(decode_ddg_url).unwrap_or_default();

            if !clean_title.is_empty() && !clean_url.is_empty() {
                results.push(SearchResult {
                    title: clean_title,
                    url: clean_url,
                    snippet: clean_snippet,
                });
            }
        }
    }

    Ok(results)
}

fn extract_between<'a>(text: &'a str, start_marker: &str, end_marker: &str) -> Option<&'a str> {
    let start_idx = text.find(start_marker)?;
    let after_marker = &text[start_idx + start_marker.len()..];
    // Skip to the next '>'
    let content_start = after_marker.find('>')? + 1;
    let content = &after_marker[content_start..];
    let end_idx = content.find(end_marker)?;
    Some(&content[..end_idx])
}

fn extract_href<'a>(text: &'a str, class_marker: &str) -> Option<&'a str> {
    let start_idx = text.find(class_marker)?;
    let before = &text[..start_idx];
    // Find the nearest href= before or after the class marker
    let after_marker = &text[start_idx..];
    if let Some(href_idx) = after_marker.find("href=\"") {
        let url_start = href_idx + 6;
        let url_text = &after_marker[url_start..];
        let url_end = url_text.find('"')?;
        return Some(&url_text[..url_end]);
    }
    // Try before the class marker
    let href_idx = before.rfind("href=\"")?;
    let url_start = href_idx + 6;
    let url_text = &before[url_start..];
    let url_end = url_text.find('"')?;
    Some(&url_text[..url_end])
}

fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    // Decode common HTML entities
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .trim()
        .to_string()
}

fn decode_ddg_url(url: &str) -> String {
    // DuckDuckGo redirects look like: //duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com&rut=...
    if let Some(uddg_start) = url.find("uddg=") {
        let encoded = &url[uddg_start + 5..];
        let end = encoded.find('&').unwrap_or(encoded.len());
        let encoded_url = &encoded[..end];
        return urlencoding::decode(encoded_url)
            .map(|s| s.into_owned())
            .unwrap_or_else(|_| encoded_url.to_string());
    }
    // If it's a plain URL, clean it up
    let cleaned = url.trim().trim_start_matches("//");
    if cleaned.starts_with("http://") || cleaned.starts_with("https://") {
        cleaned.to_string()
    } else {
        format!("https://{cleaned}")
    }
}

// ── Brave JSON parsing ───────────────────────────────────────────

fn parse_brave_json(
    body: &serde_json::Value,
    max_results: usize,
) -> anyhow::Result<Vec<SearchResult>> {
    let mut results = Vec::new();

    let web_results = body
        .get("web")
        .and_then(|w| w.get("results"))
        .and_then(|r| r.as_array());

    if let Some(items) = web_results {
        for item in items.iter().take(max_results) {
            let title = item
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let url = item
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let snippet = item
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            if !title.is_empty() && !url.is_empty() {
                results.push(SearchResult {
                    title,
                    url,
                    snippet,
                });
            }
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::{AutonomyLevel, SecurityPolicy};

    fn test_tool() -> WebSearchTool {
        let security = Arc::new(SecurityPolicy {
            autonomy: AutonomyLevel::Supervised,
            ..SecurityPolicy::default()
        });
        WebSearchTool::new(security, "duckduckgo".into(), None, 5, 15)
    }

    #[test]
    fn name_is_web_search() {
        assert_eq!(test_tool().name(), "web_search");
    }

    #[test]
    fn parameters_schema_requires_query() {
        let schema = test_tool().parameters_schema();
        assert!(schema["properties"]["query"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v.as_str() == Some("query")));
    }

    #[test]
    fn max_results_clamped() {
        let security = Arc::new(SecurityPolicy::default());
        let tool = WebSearchTool::new(security.clone(), "duckduckgo".into(), None, 50, 15);
        assert_eq!(tool.max_results, 10);
        let tool = WebSearchTool::new(security, "duckduckgo".into(), None, 0, 15);
        assert_eq!(tool.max_results, 1);
    }

    #[test]
    fn strip_html_tags_works() {
        assert_eq!(strip_html_tags("<b>Hello</b> <i>world</i>"), "Hello world");
        assert_eq!(strip_html_tags("no tags"), "no tags");
        assert_eq!(strip_html_tags("&amp; &lt;"), "& <");
    }

    #[test]
    fn format_results_formats_correctly() {
        let results = vec![SearchResult {
            title: "Example".into(),
            url: "https://example.com".into(),
            snippet: "An example page".into(),
        }];
        let output = format_results(&results);
        assert!(output.contains("1. Example"));
        assert!(output.contains("URL: https://example.com"));
        assert!(output.contains("An example page"));
    }

    #[test]
    fn decode_ddg_url_extracts_encoded() {
        let ddg = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpage&rut=abc";
        assert_eq!(decode_ddg_url(ddg), "https://example.com/page");
    }

    #[test]
    fn decode_ddg_url_passes_through_plain() {
        assert_eq!(decode_ddg_url("https://example.com"), "https://example.com");
    }

    #[test]
    fn parse_brave_json_extracts_results() {
        let json = serde_json::json!({
            "web": {
                "results": [
                    {
                        "title": "Rust Lang",
                        "url": "https://www.rust-lang.org",
                        "description": "Rust programming language"
                    },
                    {
                        "title": "Crates.io",
                        "url": "https://crates.io",
                        "description": "Rust package registry"
                    }
                ]
            }
        });
        let results = parse_brave_json(&json, 5).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Rust Lang");
        assert_eq!(results[1].url, "https://crates.io");
    }

    #[tokio::test]
    async fn blocks_readonly_mode() {
        let security = Arc::new(SecurityPolicy {
            autonomy: AutonomyLevel::ReadOnly,
            ..SecurityPolicy::default()
        });
        let tool = WebSearchTool::new(security, "duckduckgo".into(), None, 5, 15);
        let result = tool.execute(json!({"query": "rust"})).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("read-only"));
    }

    #[tokio::test]
    async fn rejects_empty_query() {
        let result = test_tool().execute(json!({"query": "  "})).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("empty"));
    }

    #[tokio::test]
    async fn blocks_rate_limited() {
        let security = Arc::new(SecurityPolicy {
            max_actions_per_hour: 0,
            ..SecurityPolicy::default()
        });
        let tool = WebSearchTool::new(security, "duckduckgo".into(), None, 5, 15);
        let result = tool.execute(json!({"query": "test"})).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("rate limit"));
    }
}
