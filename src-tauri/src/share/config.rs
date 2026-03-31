use anyhow::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OssConfig {
    access_key_id: String,
    access_key_secret: String,
    bucket: String,
    region: String,
    dist: String,
    host: String,
}

impl OssConfig {
    pub fn from_env() -> Result<Self> {
        let access_key_id = required_env("MOSAIC_OSS_ACCESSKEY_ID", "VITE_OSS_ACCESSKEY_ID")?;
        let access_key_secret =
            required_env("MOSAIC_OSS_ACCESSKEY_SECRET", "VITE_OSS_ACCESSKEY_SECRET")?;
        let bucket = required_env("MOSAIC_OSS_BUCKET", "VITE_OSS_BUCKET")?;
        let region = required_env("MOSAIC_OSS_REGION", "VITE_OSS_REGION")?;
        let dist = optional_env("MOSAIC_OSS_DIST", "VITE_OSS_DIST").unwrap_or_default();
        let host = optional_env("MOSAIC_OSS_HOST", "VITE_OSS_HOST")
            .unwrap_or_else(|| format!("https://{}.{}", bucket, endpoint_from_region(&region)));

        Ok(Self {
            access_key_id,
            access_key_secret,
            bucket,
            region,
            dist,
            host,
        })
    }

    pub fn access_key_id(&self) -> &str {
        &self.access_key_id
    }

    pub fn access_key_secret(&self) -> &str {
        &self.access_key_secret
    }

    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    pub fn endpoint(&self) -> String {
        endpoint_from_region(&self.region)
    }

    pub fn normalized_prefix(&self) -> String {
        let trimmed = self.dist.trim().trim_matches('/');
        if trimmed.is_empty() {
            String::new()
        } else {
            format!("{trimmed}/")
        }
    }

    pub fn public_base_url(&self) -> String {
        self.host.trim().trim_end_matches('/').to_string()
    }

    pub fn public_url_for_key(&self, key: &str) -> String {
        format!("{}/{}", self.public_base_url(), key.trim_start_matches('/'))
    }
}

fn required_env(primary: &str, fallback: &str) -> Result<String> {
    optional_env(primary, fallback)
        .ok_or_else(|| anyhow::anyhow!("missing OSS config: expected {} or {}", primary, fallback))
}

fn optional_env(primary: &str, fallback: &str) -> Option<String> {
    std::env::var(primary)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var(fallback)
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
}

fn endpoint_from_region(region: &str) -> String {
    let trimmed = region.trim().trim_end_matches('/');
    if trimmed.contains('.') {
        trimmed.to_string()
    } else {
        format!("{trimmed}.aliyuncs.com")
    }
}

#[cfg(test)]
mod tests {
    use super::OssConfig;

    fn demo_config() -> OssConfig {
        OssConfig {
            access_key_id: "ak".into(),
            access_key_secret: "sk".into(),
            bucket: "bucket-demo".into(),
            region: "oss-cn-hangzhou".into(),
            dist: "ai-share/".into(),
            host: "https://cdn.example.com/".into(),
        }
    }

    #[test]
    fn normalized_prefix_adds_single_trailing_slash() {
        let config = demo_config();
        assert_eq!(config.normalized_prefix(), "ai-share/");
    }

    #[test]
    fn public_url_uses_configured_host() {
        let config = demo_config();
        assert_eq!(
            config.public_url_for_key("ai-share/share-1/index.html"),
            "https://cdn.example.com/ai-share/share-1/index.html"
        );
    }

    #[test]
    fn endpoint_expands_region_to_aliyuncs_domain() {
        let config = demo_config();
        assert_eq!(config.endpoint(), "oss-cn-hangzhou.aliyuncs.com");
    }
}
