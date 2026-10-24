//! Modrinth 集成：搜索 / 版本筛选 / 下载 / 依赖自动补全
//!
//! 数据源：https://api.modrinth.com/v2（官方开放 API，无需密钥，完全去中心化）

use std::collections::{HashMap, HashSet, VecDeque};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{LauncherError, Result};
use crate::launcher::download::{DownloadTask, Downloader};
use crate::launcher::settings::Settings;

pub const MODRINTH_API: &str = "https://api.modrinth.com/v2";

// ---------------------------------------------------------------- 数据模型

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModSearchResponse {
    pub hits: Vec<ModHit>,
    #[serde(rename = "total_hits")]
    pub total_hits: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModHit {
    #[serde(rename = "project_id")]
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub downloads: u64,
    pub icon_url: Option<String>,
    pub versions: Vec<String>,
    pub categories: Vec<String>,
    #[serde(rename = "project_type")]
    pub project_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModVersion {
    pub id: String,
    pub name: String,
    #[serde(rename = "version_number")]
    pub version_number: String,
    #[serde(rename = "game_versions")]
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub files: Vec<ModFile>,
    pub dependencies: Vec<ModDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModFile {
    pub url: String,
    pub filename: String,
    pub size: u64,
    pub hashes: HashMap<String, String>,
    pub primary: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModDependency {
    #[serde(rename = "project_id")]
    pub project_id: Option<String>,
    #[serde(rename = "version_id")]
    pub version_id: Option<String>,
    #[serde(rename = "dependency_type")]
    pub dependency_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstalledMod {
    pub project_id: String,
    pub title: String,
    pub filename: String,
    pub size: u64,
    /// 是否为自动补全的依赖
    pub is_dependency: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallModReport {
    pub installed: Vec<InstalledMod>,
    /// 未能自动安装的 required 依赖（如无匹配版本）
    pub unresolved: Vec<String>,
}


// ---------------------------------------------------------------- 客户端

pub struct ModrinthClient {
    client: Client,
}

impl ModrinthClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent(concat!("SoloMC/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("http client");
        Self { client }
    }

    async fn get_json(&self, url: &str) -> Result<serde_json::Value> {
        let resp = self.client.get(url).send().await?;
        if !resp.status().is_success() {
            return Err(LauncherError::HttpStatus(resp.status()));
        }
        Ok(resp.json().await?)
    }

    /// 搜索模组
    pub async fn search(
        &self,
        query: &str,
        game_version: Option<&str>,
        loader: Option<&str>,
        limit: u32,
    ) -> Result<Vec<ModHit>> {
        let mut facets: Vec<Vec<String>> = vec![vec!["project_type:mod".to_string()]];
        if let Some(gv) = game_version {
            facets.push(vec![format!("versions:{gv}")]);
        }
        if let Some(l) = loader {
            facets.push(vec![format!("categories:{l}")]);
        }
        let facets_json = json!(facets).to_string();

        let mut url = format!("{MODRINTH_API}/search?limit={limit}&facets={}", url::form_urlencoded::byte_serialize(facets_json.as_bytes()).collect::<String>());
        if !query.trim().is_empty() {
            url.push_str(&format!("&query={}", url::form_urlencoded::byte_serialize(query.trim().as_bytes()).collect::<String>()));
        }
        let v = self.get_json(&url).await?;
        let resp: ModSearchResponse = serde_json::from_value(v)?;
        Ok(resp.hits)
    }

    /// 某项目在指定 MC 版本 + 加载器下的可用版本
    pub async fn project_versions(
        &self,
        project_id: &str,
        game_version: Option<&str>,
        loader: Option<&str>,
    ) -> Result<Vec<ModVersion>> {
        let mut params = Vec::new();
        if let Some(gv) = game_version {
            params.push(format!(
                "game_versions={}",
                url::form_urlencoded::byte_serialize(json!([gv]).to_string().as_bytes()).collect::<String>()
            ));
        }
        if let Some(l) = loader {
            params.push(format!(
                "loaders={}",
                url::form_urlencoded::byte_serialize(json!([l]).to_string().as_bytes()).collect::<String>()
            ));
        }
        let qs = if params.is_empty() {
            String::new()
        } else {
            format!("?{}", params.join("&"))
        };
        let url = format!("{MODRINTH_API}/project/{project_id}/version{qs}");
        let v = self.get_json(&url).await?;
        let versions: Vec<ModVersion> = serde_json::from_value(v)?;
        // 优先加载器/版本匹配的，其次按发布时间（接口默认倒序）
        Ok(versions)
    }

    /// 安装模组到指定实例，并递归补全 required 依赖（防环）
    pub async fn install_mod(
        &self,
        settings: &Settings,
        project_id: &str,
        version_id: &str,
        instance_id: &str,
        concurrency: usize,
        install_deps: bool,
    ) -> Result<InstallModReport> {
        let mods_dir = settings
            .versions_dir()
            .join(instance_id)
            .join("mods");
        std::fs::create_dir_all(&mods_dir)?;

        let dl = Downloader::new(concurrency);
        let mut installed: Vec<InstalledMod> = Vec::new();
        let mut unresolved: Vec<String> = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        // queue: (project_id, version_id)
        let mut queue: VecDeque<(String, Option<String>)> = VecDeque::new();
        queue.push_back((project_id.to_string(), Some(version_id.to_string())));

        // 记录标题（用于展示）
        let mut titles: HashMap<String, String> = HashMap::new();

        while let Some((pid, vid)) = queue.pop_front() {
            if visited.contains(&pid) {
                continue;
            }
            visited.insert(pid.clone());

            // 解析出要下载的版本
            let version: ModVersion = if let Some(vid) = &vid {
                let v = self.get_json(&format!("{MODRINTH_API}/version/{vid}")).await?;
                serde_json::from_value(v)?
            } else {
                // 未指定版本：查询匹配的版本（无筛选条件，取第一个）
                let versions = self
                    .project_versions(&pid, None, None)
                    .await?;
                match versions.into_iter().next() {
                    Some(v) => v,
                    None => {
                        unresolved.push(pid);
                        continue;
                    }
                }
            };

            let title = if pid == project_id {
                // 根模组标题：从版本接口拿不到项目名，用版本名
                version.name.clone()
            } else {
                titles.get(&pid).cloned().unwrap_or_else(|| pid.clone())
            };

            // 下载该版本所有文件到 mods 目录
            let mut tasks = Vec::new();
            for f in &version.files {
                let sha1 = f.hashes.get("sha1").cloned();
                tasks.push(DownloadTask {
                    url: f.url.clone(),
                    dest: mods_dir.join(&f.filename),
                    sha1,
                    size: Some(f.size),
                    label: f.filename.clone(),
                });
                installed.push(InstalledMod {
                    project_id: pid.clone(),
                    title: title.clone(),
                    filename: f.filename.clone(),
                    size: f.size,
                    is_dependency: pid != project_id,
                });
            }
            if !tasks.is_empty() {
                dl.download_all(tasks).await?;
            }

            // 补全依赖
            if install_deps {
                for dep in &version.dependencies {
                    if dep.dependency_type != "required" {
                        continue;
                    }
                    let Some(dep_pid) = &dep.project_id else {
                        continue;
                    };
                    if visited.contains(dep_pid) {
                        continue;
                    }
                    // 记录依赖项目名（通过搜索接口轻量获取，失败忽略）
                    if !titles.contains_key(dep_pid) {
                        if let Ok(hits) = self.search(dep_pid, None, None, 1).await {
                            if let Some(h) = hits.first() {
                                titles.insert(dep_pid.clone(), h.title.clone());
                            }
                        }
                    }
                    queue.push_back((dep_pid.clone(), dep.version_id.clone()));
                }
            }
        }

        Ok(InstallModReport {
            installed,
            unresolved,
        })
    }
}
