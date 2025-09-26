use std::{collections::HashMap, fmt::Display, path::Path};

use clap::ValueEnum;
use serde::Deserialize;

use crate::network::{display_json_value, download_file};

#[derive(Debug, Clone, ValueEnum)]
pub enum SearchIndex {
    Relevance,
    Downloads,
    Follows,
    Newest,
}

impl Display for SearchIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = format!("{self:?}").to_lowercase();
        write!(f, "{s}")
    }
}

// https://docs.modrinth.com/api/operations/searchprojects/
#[derive(Deserialize)]
pub struct SearchResponse(serde_json::Value);

impl Display for SearchResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let hits = self.0["hits"].as_array().unwrap();

        // The fields we want to display from each hit
        let fields = [
            "title",
            "slug",
            "project_id",
            "author",
            "downloads",
            "description",
            "categories",
            "versions",
        ];

        for hit in hits {
            for field in &fields {
                writeln!(f, "{}", display_json_value(hit, field))?;
            }
            writeln!(f, "=======================================")?;
        }

        Ok(())
    }
}

// https://docs.modrinth.com/api/operations/getprojectversions/
#[derive(Deserialize)]
pub struct ProjectVersionsResponse(serde_json::Value);

impl Display for ProjectVersionsResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let versions = self.0.as_array().unwrap();

        // The fields we want to display from each version
        let fields = [
            "version_number",
            "id",
            "dependencies",
            "game_versions",
            "version_type",
            "loaders",
        ];

        for version in versions {
            for field in &fields {
                writeln!(f, "{}", display_json_value(version, field))?;
            }
            writeln!(f, "=======================================")?;
        }

        Ok(())
    }
}

pub struct ModVersion {
    pub project_id: String,
    // Prefer using the version name over the version number.
    // Example: Multiple versions might share the version number `1.8.2`,
    // but have distinct names such as `1.8.2-1.21.5 - Fabric` or `1.8.2-1.21.6 - Fabric`.
    pub version_name: String,
    pub hash: String,
    pub file_url: String,
    pub file_name: String,
}

/// Searches for mods on Modrinth with the given query and facets.
/// Will always add server_side facets, but this is fragile becuase of Modrinth API.
pub async fn search(
    client: &reqwest::Client,
    query: &str,
    facets: &[&str],
    index: Option<SearchIndex>,
    limit: Option<usize>,
) -> anyhow::Result<SearchResponse> {
    let mut builder = client.get("https://api.modrinth.com/v2/search");

    builder = builder.query(&[("query", query)]);

    let joined = facets
        .iter()
        .map(|f| format!(",[\"{f}\"]"))
        .collect::<Vec<_>>()
        .join("");

    let facets = format!("[[\"server_side:required\",\"server_side:optional\"]{joined}]");

    builder = builder.query(&[("facets", facets)]);

    if let Some(i) = index {
        builder = builder.query(&[("index", i.to_string())]);
    }
    if let Some(l) = limit {
        builder = builder.query(&[("limit", l.to_string())]);
    }

    let result = builder.send().await?.error_for_status()?;

    Ok(serde_json::from_str(&result.text().await?)?)
}

pub async fn get_project_versions(
    client: &reqwest::Client,
    project_slug: &str,
    featured: bool,
) -> anyhow::Result<ProjectVersionsResponse> {
    let mut builder = client.get(format!(
        "https://api.modrinth.com/v2/project/{project_slug}/version"
    ));

    // Only filter by Fabric loader
    builder = builder.query(&[
        ("loaders", "[\"fabric\"]"),
        ("featured", &featured.to_string()),
    ]);

    let result = builder.send().await?.error_for_status()?;
    let response: ProjectVersionsResponse = serde_json::from_str(&result.text().await?)?;

    Ok(response)
}

pub async fn download_version(
    client: &reqwest::Client,
    version_id: &str,
    save_dir_path: impl AsRef<Path>,
) -> anyhow::Result<String> {
    let result = client
        .get(format!("https://api.modrinth.com/v2/version/{version_id}"))
        .send()
        .await?
        .error_for_status()?;

    let response: serde_json::Value = serde_json::from_str(&result.text().await?)?;

    let files = response["files"].as_array().unwrap();

    if files.len() > 1 {
        println!(
            "Multiple files found for version {version_id}. Downloading the first on the list..."
        );
    }

    let url = files[0]["url"].as_str().unwrap();
    let file_name = files[0]["filename"].as_str().unwrap();
    let file_path = save_dir_path.as_ref().join(file_name);
    download_file(client, &url, &file_path).await?;

    Ok(file_name.to_string())
}

// https://docs.modrinth.com/api/operations/getprojects/
// Cannot just return vec like other functions. This response will not guarantee the order.
/// Returns a map of project IDs to slugs.
pub async fn get_project_slug_map<I, S>(
    client: &reqwest::Client,
    project_ids: I,
) -> anyhow::Result<HashMap<String, String>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let project_ids = format!(
        "[{}]",
        project_ids
            .into_iter()
            .map(|id| format!("\"{}\"", id.as_ref()))
            .collect::<Vec<_>>()
            .join(",")
    );

    let result = client
        .get("https://api.modrinth.com/v2/projects")
        .query(&[("ids", project_ids)])
        .send()
        .await?
        .error_for_status()?;

    let response: serde_json::Value = serde_json::from_str(&result.text().await?)?;

    let array = response.as_array().unwrap();

    let slug_map = array
        .iter()
        .map(|project| {
            let id = project["id"].as_str().unwrap().to_string();
            let slug = project["slug"].as_str().unwrap().to_string();
            (id, slug)
        })
        .collect::<HashMap<_, _>>();

    Ok(slug_map)
}

// https://docs.modrinth.com/api/operations/versionsfromhashes/
pub async fn get_versions(
    client: &reqwest::Client,
    jar_hashes: &[impl AsRef<str>],
) -> anyhow::Result<Vec<ModVersion>> {
    let request_body: serde_json::Value = serde_json::json!({
        "hashes": jar_hashes.iter().map(|h| h.as_ref()).collect::<Vec<_>>(),
        "algorithm": "sha1",
    });

    let result = client
        .post("https://api.modrinth.com/v2/version_files")
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await?
        .error_for_status()?;

    let response: serde_json::Value = serde_json::from_str(&result.text().await?)?;
    parse_version_response(response, jar_hashes)
}

// https://docs.modrinth.com/api/operations/getlatestversionfromhash/
pub async fn get_latest_versions(
    client: &reqwest::Client,
    jar_hashes: &[impl AsRef<str>],
    game_versions: &[impl AsRef<str>],
) -> anyhow::Result<Vec<ModVersion>> {
    let request_body: serde_json::Value = serde_json::json!({
        "hashes": jar_hashes.iter().map(|h| h.as_ref()).collect::<Vec<_>>(),
        "algorithm": "sha1",
        "loaders": ["fabric"], // hardcoded fabric
        "game_versions": game_versions.iter().map(|v| v.as_ref()).collect::<Vec<_>>()
    });

    let result = client
        .post("https://api.modrinth.com/v2/version_files/update")
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await?
        .error_for_status()?;

    let response: serde_json::Value = serde_json::from_str(&result.text().await?)?;
    parse_version_response(response, jar_hashes)
}

fn parse_version_response(
    response: serde_json::Value,
    jar_hashes: &[impl AsRef<str>],
) -> anyhow::Result<Vec<ModVersion>> {
    let response_map = response.as_object().unwrap();

    let versions = jar_hashes
        .iter()
        .map(|hash| {
            let value = &response_map[hash.as_ref()];
            let project_id = value["project_id"].as_str().unwrap().to_string();
            let version_name = value["name"].as_str().unwrap_or("N/A").to_string();
            let files = value["files"].as_array().unwrap();

            if files.len() > 1 {
                println!("Multiple files found for version {version_name}. Using the first one.");
            }

            let file = &files[0];
            let hash = file["hashes"]["sha1"].as_str().unwrap().to_string();
            let file_url = file["url"].as_str().unwrap().to_string();
            let file_name = file["filename"].as_str().unwrap().to_string();

            ModVersion {
                project_id,
                version_name,
                hash,
                file_url,
                file_name,
            }
        })
        .collect();

    Ok(versions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_search_with_filters() {
        let client = reqwest::Client::new();
        let query = "map";
        let facets = ["license:mit", "project_type:mod"];

        let result = search(
            &client,
            query,
            &facets,
            Some(SearchIndex::Downloads),
            Some(4),
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_search_without_filters() {
        let client = reqwest::Client::new();
        let query = "map";

        let result = search(&client, query, &[], None, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_project_versions() {
        let client = reqwest::Client::new();
        let project_slug = "fabric-api";
        let result = get_project_versions(&client, project_slug, false).await;

        assert!(result.is_ok());
    }
}
