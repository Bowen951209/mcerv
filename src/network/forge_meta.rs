use reqwest::Client;
use roxmltree::Document;

pub async fn versions(client: &Client) -> anyhow::Result<String> {
    let url = "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml";
    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to fetch {}: {}", url, response.status());
    }

    let text = response.text().await?;
    let doc = Document::parse(&text)?;
    let versions = doc
        .descendants()
        .filter(|node| node.has_tag_name("version"))
        .filter_map(|node| node.text().map(String::from))
        .collect::<Vec<_>>();

    Ok(versions.join("\n"))
}
