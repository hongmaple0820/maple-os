use anyhow::Result;

#[derive(Clone)]
pub struct WebDavClient {
    base_url: String,
    username: String,
    password: String,
    client: reqwest::Client,
}

impl WebDavClient {
    pub fn new(base_url: String, username: String, password: String) -> Self {
        Self {
            base_url,
            username,
            password,
            client: reqwest::Client::new(),
        }
    }

    pub async fn put(&self, path: &str, data: &[u8]) -> Result<()> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.client
            .put(&url)
            .basic_auth(&self.username, Some(&self.password))
            .body(data.to_vec())
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("WebDAV PUT failed: {}", resp.status());
        }
        Ok(())
    }

    pub async fn get(&self, path: &str) -> Result<Vec<u8>> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.client
            .get(&url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("WebDAV GET failed: {}", resp.status());
        }

        Ok(resp.bytes().await?.to_vec())
    }

    pub async fn exists(&self, path: &str) -> Result<bool> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.client
            .head(&url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await?;

        Ok(resp.status().is_success())
    }
}
