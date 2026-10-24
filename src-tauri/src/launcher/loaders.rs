//! 加载器安装统一入口：Fabric / Forge / NeoForge
//!
//! 全部走官方公开接口（Fabric Meta / Forge Maven / NeoForge Maven），
//! 与去中心化目标一致：不依赖任何第三方整合平台。

use serde::{Deserialize, Serialize};

use crate::error::{LauncherError, Result};
use crate::launcher::install::{GameInstaller, ProgressFn};
use crate::launcher::meta::VersionMeta;
use crate::launcher::settings::{Instance, Settings};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoaderKind {
    Fabric,
    Forge,
    NeoForge,
}

impl LoaderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            LoaderKind::Fabric => "fabric",
            LoaderKind::Forge => "forge",
            LoaderKind::NeoForge => "neoforge",
        }
    }
}

impl std::fmt::Display for LoaderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for LoaderKind {
    type Err = LauncherError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "fabric" => Ok(LoaderKind::Fabric),
            "forge" => Ok(LoaderKind::Forge),
            "neoforge" => Ok(LoaderKind::NeoForge),
            _ => Err(LauncherError::msg(format!("未知加载器: {s}"))),
        }
    }
}

/// 某 MC 版本可用的加载器版本列表（已按推荐排序）
pub async fn loader_versions(
    loader: LoaderKind,
    mc_version: &str,
) -> Result<Vec<String>> {
    let client = reqwest::Client::new();
    match loader {
        LoaderKind::Fabric => crate::launcher::fabric::loader_versions(&client, mc_version).await,
        LoaderKind::Forge => crate::launcher::forge::loader_versions(&client, mc_version).await,
        LoaderKind::NeoForge => crate::launcher::neoforge::loader_versions(&client, mc_version).await,
    }
}

/// 安装加载器实例（版本隔离目录），返回实例信息
pub async fn install_loader(
    settings: &Settings,
    loader: LoaderKind,
    mc_version: &str,
    loader_version: &str,
    concurrency: usize,
    on_progress: Option<ProgressFn>,
) -> Result<Instance> {
    let client = reqwest::Client::new();
    let profile: serde_json::Value = match loader {
        LoaderKind::Fabric => {
            crate::launcher::fabric::profile_json(&client, mc_version, loader_version).await?
        }
        LoaderKind::Forge => {
            crate::launcher::forge::profile_json(&client, mc_version, loader_version).await?
        }
        LoaderKind::NeoForge => {
            crate::launcher::neoforge::profile_json(&client, loader_version).await?
        }
    };

    let meta: VersionMeta = serde_json::from_value(profile)?;
    let instance_id = meta.id.clone();

    let mut installer = GameInstaller::new(settings.clone());
    if let Some(f) = on_progress {
        installer = installer.with_progress(f);
    }
    let merged = installer.resolve_merged(&meta).await?;
    installer
        .install_meta(&instance_id, &meta, &merged, concurrency)
        .await?;

    Ok(Instance {
        id: instance_id.clone(),
        version_id: instance_id,
        mc_version: mc_version.to_string(),
        loader: Some(loader.to_string()),
        loader_version: Some(loader_version.to_string()),
    })
}

/// 列出本地已安装的实例（扫描 versions 目录下的 *.json 并读取 id）
pub fn list_instances(settings: &Settings) -> Vec<Instance> {
    let dir = settings.versions_dir();
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            continue;
        }
        let Some(ext) = p.extension() else { continue };
        if ext != "json" {
            continue;
        }
        let id = p
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        // 读 json 提取 mc 版本与加载器信息
        let info = std::fs::read_to_string(&p).ok();
        let (mc_version, loader, loader_version) = match info {
            Some(s) => {
                let v: serde_json::Value = serde_json::from_str(&s).unwrap_or(serde_json::Value::Null);
                let mc = v
                    .get("inheritsFrom")
                    .or_else(|| v.get("id"))
                    .and_then(|x| x.as_str())
                    .unwrap_or(&id)
                    .to_string();
                let loader_hint = detect_loader(&s);
                loader_hint
                    .map(|(l, lv)| (mc.clone(), Some(l), lv))
                    .unwrap_or((mc, None, None))
            }
            None => (id.clone(), None, None),
        };
        out.push(Instance {
            id: id.clone(),
            version_id: id,
            mc_version,
            loader,
            loader_version,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// 从版本 json 文本粗检测加载器
fn detect_loader(json: &str) -> Option<(String, Option<String>)> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let libs = v.get("libraries")?.as_array()?;
    for lib in libs {
        let name = lib.get("name")?.as_str()?;
        let lower = name.to_lowercase();
        if lower.contains("net.fabricmc:fabric-loader") {
            let ver = lib.get("version").and_then(|x| x.as_str()).map(|s| s.to_string());
            return Some(("fabric".to_string(), ver));
        }
        if lower.contains("net.minecraftforge:forge") || lower.contains("net.minecraftforge:fmlloader") {
            let ver = name.rsplit(':').next().map(|s| s.to_string());
            return Some(("forge".to_string(), ver));
        }
        if lower.contains("net.neoforged:neoforge") || lower.contains("net.neoforged:fancymodloader") {
            let ver = name.rsplit(':').next().map(|s| s.to_string());
            return Some(("neoforge".to_string(), ver));
        }
    }
    None
}
