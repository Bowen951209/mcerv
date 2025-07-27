use std::{fmt::Display, path::Path};

use serde::Deserialize;

use crate::network::{display_json_value, download_file};

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
            "name",
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

pub async fn search(
    client: &reqwest::Client,
    query: &str,
    facets: &[&str],
    index: Option<&str>,
    limit: Option<&str>,
) -> anyhow::Result<SearchResponse> {
    let mut builder = client.get("https://api.modrinth.com/v2/search");

    builder = builder.query(&[("query", query)]);

    // Facets always include the Fabric category
    let joined = facets
        .iter()
        .map(|f| format!(",[\"{}\"]", f))
        .collect::<Vec<_>>()
        .join("");

    let facets = format!("[[\"categories:fabric\"]{joined}]");

    builder = builder.query(&[("facets", facets)]);

    if let Some(i) = index {
        builder = builder.query(&[("index", i)]);
    }
    if let Some(l) = limit {
        builder = builder.query(&[("limit", l)]);
    }

    let result = builder.send().await?.error_for_status()?;

    Ok(serde_json::from_str(&result.text().await?)?)
}

pub async fn get_project_versions(
    client: &reqwest::Client,
    project_slug: &str,
    game_versions: &[&str],
    featured: Option<bool>,
) -> anyhow::Result<ProjectVersionsResponse> {
    let mut builder = client.get(format!(
        "https://api.modrinth.com/v2/project/{}/version",
        project_slug
    ));

    // Only filter by Fabric loader
    builder = builder.query(&[("loaders", "fabric")]);

    let joined = game_versions
        .iter()
        .map(|gv| format!("[\"{}\"]", gv))
        .collect::<Vec<_>>()
        .join(",");

    builder = builder.query(&[("game_versions", format!("[{joined}]"))]);

    if let Some(f) = featured {
        builder = builder.query(&[("featured", &f.to_string())]);
    }

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
            "Multiple files found for version {}. Downloading the first on the list...",
            version_id
        );
    }

    let url = files[0]["url"].as_str().unwrap();
    let file_name = files[0]["filename"].as_str().unwrap();
    let file_path = save_dir_path.as_ref().join(file_name);
    download_file(client, url, &file_path).await?;

    Ok(file_name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_search_with_filters() {
        let client = reqwest::Client::new();
        let query = "map";
        let facets = ["license:mit", "project_type:mod"];

        let result = search(&client, query, &facets, Some("downloads"), Some("4")).await;
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
    async fn test_get_project_versions_with_filters() {
        let client = reqwest::Client::new();
        let project_slug = "fabric-api";
        let game_versions = ["1.21", "1.21.1"];
        let featured = true;
        let result =
            get_project_versions(&client, project_slug, &game_versions, Some(featured)).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_project_versions_without_filters() {
        let client = reqwest::Client::new();
        let project_slug = "fabric-api";

        let result = get_project_versions(&client, project_slug, &[], None).await;
        assert!(result.is_ok());
    }
}
