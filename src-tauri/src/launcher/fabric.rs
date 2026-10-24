//! Fabric 加载器：从官方 Fabric Meta API 获取版本与 profile json
//! https://meta.fabricmc.net/v2/versions/loader/{mc_version}

use reqwest::Client;
use serde::Deserialize;

use crate::error::{LauncherError, Result};

#[derive(Debug, Clone, Deserialize)]
pub struct FabricArtifact {
    pub version: String,
    pub stable: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FabricLoaderEntry {
    pub loader: FabricArtifact,
    pub installer: FabricArtifact,
}

/// 获取某 MC 版本可用的 Fabric 加载器版本（稳定版优先排序）
pub async fn loader_versions(client: &Client, mc_version: &str) -> Result<Vec<String>> {
    let url = format!("https://meta.fabricmc.net/v2/versions/loader/{mc_version}");
    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        return Err(LauncherError::HttpStatus(resp.status()));
    }
    let entries: Vec<FabricLoaderEntry> = resp.json().await?;
    let mut stable: Vec<String> = Vec::new();
    let mut others: Vec<String> = Vec::new();
    for e in entries {
        if e.loader.stable.unwrap_or(false) {
            stable.push(e.loader.version);
        } else {
            others.push(e.loader.version);
        }
    }
    if stable.is_empty() {
        Ok(others)
    } else {
        Ok(stable)
    }
}

/// 拉取 Fabric profile json（继承原版，含 loader libraries）
pub async fn profile_json(
    client: &Client,
    mc_version: &str,
    loader_version: &str,
) -> Result<serde_json::Value> {
    // 先查该 loader 版本对应的 installer 版本
    let meta_url = format!(
        "https://meta.fabricmc.net/v2/versions/loader/{mc_version}/{loader_version}"
    );
    let resp = client.get(&meta_url).send().await?;
    if !resp.status().is_success() {
        return Err(LauncherError::HttpStatus(resp.status()));
    }
    let entry: FabricLoaderEntry = resp.json().await?;
    let installer = &entry.installer.version;

    let profile_url = format!(
        "https://meta.fabricmc.net/v2/versions/loader/{mc_version}/{loader_version}/{installer}/profile/json"
    );
    let resp = client.get(&profile_url).send().await?;
    if !resp.status().is_success() {
        return Err(LauncherError::HttpStatus(resp.status()));
    }
    Ok(resp.json().await?)
}
