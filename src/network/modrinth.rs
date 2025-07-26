use reqwest::Client;

#[derive(serde::Deserialize)]
#[allow(unused)]
pub struct SearchResponse {
    pub hits: Vec<Hit>,
    offset: u32,
    limit: u32,
    total_hits: u32,
}

impl SearchResponse {
    pub fn print_table(&self) {
        // Don't use prettytable here because long descriptions can break the table layout.
        for hit in &self.hits {
            println!("Title: {}", hit.title);
            println!("Author: {}", hit.author);
            println!("Downloads: {}", hit.downloads);
            println!("Description: {}", hit.description);
            println!("Categories: {:?}", hit.categories);
            println!("Versions: {:?}", hit.versions);
            println!("=======================================");
        }
    }
}

#[derive(serde::Deserialize)]
#[allow(unused)]
pub struct Hit {
    pub project_id: String,
    pub project_type: String,
    pub slug: String,
    pub author: String,
    pub title: String,
    pub description: String,
    pub categories: Vec<String>,
    pub display_categories: Vec<String>,
    pub versions: Vec<String>,
    pub downloads: u64,
    pub follows: u64,
    pub icon_url: String,
    pub date_created: String,
    pub date_modified: String,
    pub latest_version: String,
    pub license: String,
    pub client_side: String,
    pub server_side: String,
    pub gallery: Vec<String>,
    pub featured_gallery: serde_json::Value,
    pub color: u32,
}

pub async fn search(
    query: &str,
    facets: Option<&str>,
    index: Option<&str>,
    limit: Option<&str>,
) -> anyhow::Result<SearchResponse> {
    let client = Client::new();

    let mut builder = client.get("https://api.modrinth.com/v2/search");

    builder = builder.query(&[("query", query)]);
    if let Some(f) = facets {
        builder = builder.query(&[("facets", f)]);
    }
    if let Some(i) = index {
        builder = builder.query(&[("index", i)]);
    }
    if let Some(l) = limit {
        builder = builder.query(&[("limit", l)]);
    }

    let result = builder.send().await?.error_for_status()?;

    let response: SearchResponse = serde_json::from_str(&result.text().await?)?;

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_search() {
        let query = "map";
        let facets = "[[\"categories:fabric\"]]";

        let result = search(query, Some(facets), Some("downloads"), Some("4")).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_search_no_filters() {
        let query = "map";

        let result = search(query, None, None, None).await;
        assert!(result.is_ok());
    }
}
