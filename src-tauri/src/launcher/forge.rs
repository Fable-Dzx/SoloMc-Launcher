//! Forge 加载器：promotions 列表 + Maven profile json（现代版本 1.13+）
//!
//! - 版本列表: https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json
//! - profile:  https://maven.minecraftforge.net/net/minecraftforge/forge/{mc}-{ver}/forge-{mc}-{ver}.json

use std::collections::HashMap;

use reqwest::Client;
use serde::Deserialize;

use crate::error::{LauncherError, Result};

#[derive(Debug, Deserialize)]
struct Promotions {
    #[serde(default)]
    promos: HashMap<String, String>,
}

/// 获取某 MC 版本可用的 Forge 版本（latest 优先，其次 recommended）
pub async fn loader_versions(client: &Client, mc_version: &str) -> Result<Vec<String>> {
    let url = "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(LauncherError::HttpStatus(resp.status()));
    }
    let promos: Promotions = resp.json().await?;
    let prefix = format!("{mc_version}-");

    let mut latest = None;
    let mut recommended = None;
    let mut others: Vec<String> = Vec::new();
    for (k, v) in &promos.promos {
        if let Some(suffix) = k.strip_prefix(&prefix) {
            match suffix {
                "latest" => latest = Some(v.clone()),
                "recommended" => recommended = Some(v.clone()),
                _ => {}
            }
        }
    }
    if let Some(v) = &latest {
        others.push(v.clone());
    }
    if let Some(v) = &recommended {
        if others.first() != Some(v) {
            others.push(v.clone());
        }
    }
    // 其它出现在列表中的版本号一并返回（去重）
    let mut seen: Vec<String> = Vec::new();
    for v in others {
        if !seen.contains(&v) {
            seen.push(v);
        }
    }
    Ok(seen)
}

/// 拉取 Forge profile json（现代版本为完整版本定义，不继承）
pub async fn profile_json(
    client: &Client,
    mc_version: &str,
    forge_version: &str,
) -> Result<serde_json::Value> {
    let url = format!(
        "https://maven.minecraftforge.net/net/minecraftforge/forge/{mc_version}-{forge_version}/forge-{mc_version}-{forge_version}.json"
    );
    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        return Err(LauncherError::HttpStatus(resp.status()));
    }
    Ok(resp.json().await?)
}
