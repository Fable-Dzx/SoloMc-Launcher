//! NeoForge 加载器：官方 API + Maven profile json
//!
//! - 版本列表: https://neoforged.net/api/minecraft/{mc}/versions （fallback: maven-metadata.xml）
//! - profile:  https://maven.neoforged.net/releases/net/neoforged/neoforge/{ver}/neoforge-{ver}.json

use reqwest::Client;

use crate::error::{LauncherError, Result};

/// 从 NeoForge 版本号推断对应 MC 版本：20.4.237 -> 1.20.4，21.1.130 -> 1.21.1
pub fn neo_to_mc(version: &str) -> Option<String> {
    let mut parts = version.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    if major < 20 {
        return None;
    }
    if minor == 0 {
        Some(format!("1.{major}"))
    } else {
        Some(format!("1.{major}.{minor}"))
    }
}

/// 获取某 MC 版本可用的 NeoForge 版本（降序）
pub async fn loader_versions(client: &Client, mc_version: &str) -> Result<Vec<String>> {
    // 首选官方 API
    let api = format!("https://neoforged.net/api/minecraft/{mc_version}/versions");
    match client.get(&api).send().await {
        Ok(resp) if resp.status().is_success() => {
            let text = resp.text().await?;
            if let Some(list) = parse_version_json(&text) {
                return Ok(list);
            }
            // JSON 解析失败则尝试按行提取
            let fallback = extract_versions_text(&text);
            if !fallback.is_empty() {
                return Ok(fallback);
            }
        }
        Ok(resp) => log::warn!("neoforged API {} -> {}", api, resp.status()),
        Err(e) => log::warn!("neoforged API 请求失败: {e}"),
    }

    // fallback: maven-metadata.xml
    let meta_url = "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml";
    let resp = client.get(meta_url).send().await?;
    if !resp.status().is_success() {
        return Err(LauncherError::HttpStatus(resp.status()));
    }
    let xml = resp.text().await?;
    let mut out = Vec::new();
    for version in extract_xml_versions(&xml) {
        if let Some(mapped) = neo_to_mc(&version) {
            if mapped == mc_version {
                out.push(version);
            }
        }
    }
    // 版本号降序（语义化）
    out.sort_by(|a, b| semver_compare(b, a));
    Ok(out)
}

/// 宽松解析 neoforged.net API 响应（可能是数组或含 versions 字段的对象）
fn parse_version_json(text: &str) -> Option<Vec<String>> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let arr = v.as_array().cloned().or_else(|| {
        v.get("versions")
            .and_then(|x| x.as_array())
            .cloned()
            .or_else(|| v.get("gameVersions").and_then(|x| x.as_array()).cloned())
    })?;
    let mut out = Vec::new();
    for item in arr {
        if let Some(s) = item.as_str() {
            out.push(s.to_string());
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// 文本里提取形如 "21.1.130" 的版本号
fn extract_versions_text(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for tok in text.split(|c: char| !c.is_ascii_digit() && c != '.') {
        let tok = tok.trim_matches('.');
        if tok.is_empty() {
            continue;
        }
        if tok.split('.').count() >= 3
            && tok.chars().all(|c| c.is_ascii_digit() || c == '.')
            && tok.split('.').all(|p| !p.is_empty())
        {
            out.push(tok.to_string());
        }
    }
    out.sort_by(|a, b| semver_compare(b, a));
    out.dedup();
    out
}

/// 提取 maven-metadata.xml 中的 <version> 标签
fn extract_xml_versions(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<version>") {
        let after = &rest[start + "<version>".len()..];
        if let Some(end) = after.find("</version>") {
            let v = &after[..end];
            if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == '.') {
                out.push(v.to_string());
            }
            rest = &after[end + "</version>".len()..];
        } else {
            break;
        }
    }
    out
}

fn semver_compare(a: &str, b: &str) -> std::cmp::Ordering {
    let pa: Vec<u32> = a
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect();
    let pb: Vec<u32> = b
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect();
    for i in 0..pa.len().max(pb.len()) {
        let x = pa.get(i).copied().unwrap_or(0);
        let y = pb.get(i).copied().unwrap_or(0);
        match x.cmp(&y) {
            std::cmp::Ordering::Equal => continue,
            ord => return ord,
        }
    }
    std::cmp::Ordering::Equal
}

/// 拉取 NeoForge profile json
pub async fn profile_json(
    client: &Client,
    neoforge_version: &str,
) -> Result<serde_json::Value> {
    let url = format!(
        "https://maven.neoforged.net/releases/net/neoforged/neoforge/{neoforge_version}/neoforge-{neoforge_version}.json"
    );
    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        return Err(LauncherError::HttpStatus(resp.status()));
    }
    Ok(resp.json().await?)
}
