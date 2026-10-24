//! 版本安装：合并继承链、下载 client/assets/libraries、解压 natives

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

use crate::error::{LauncherError, Result};
use crate::launcher::download::{DownloadTask, Downloader};
use crate::launcher::meta::{OsKind, VersionManifest, VersionMeta};
use crate::launcher::settings::Settings;

pub const VERSION_MANIFEST_URL: &str =
    "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";

/// 安装进度回调（label, 当前字节, 总字节）
pub type ProgressFn = Arc<dyn Fn(String, u64, u64) + Send + Sync>;

pub struct GameInstaller {
    pub settings: Settings,
    pub client: reqwest::Client,
    on_progress: Option<ProgressFn>,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct InstallSummary {
    pub version_id: String,
    pub files_downloaded: usize,
    pub files_skipped: usize,
    pub assets_count: usize,
    pub libraries_count: usize,
    pub natives_extracted: bool,
}

impl GameInstaller {
    pub fn new(settings: Settings) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(concat!("SoloMC/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("http client");
        Self {
            settings,
            client,
            on_progress: None,
        }
    }

    pub fn with_progress(mut self, f: ProgressFn) -> Self {
        self.on_progress = Some(f);
        self
    }

    fn progress(&self, label: &str, done: u64, total: u64) {
        if let Some(f) = &self.on_progress {
            f(label.to_string(), done, total);
        }
    }

    // ---------------------------------------------------------------- 元数据

    pub async fn fetch_manifest(&self) -> Result<VersionManifest> {
        let resp = self.client.get(VERSION_MANIFEST_URL).send().await?;
        if !resp.status().is_success() {
            return Err(LauncherError::HttpStatus(resp.status()));
        }
        Ok(resp.json().await?)
    }

    pub async fn fetch_version_json(&self, url: &str) -> Result<VersionMeta> {
        let resp = self.client.get(url).send().await?;
        if !resp.status().is_success() {
            return Err(LauncherError::HttpStatus(resp.status()));
        }
        Ok(resp.json().await?)
    }

    /// 从清单中找某版本的 json url
    pub async fn find_version_url(&self, manifest: &VersionManifest, id: &str) -> Result<String> {
        manifest
            .versions
            .iter()
            .find(|v| v.id == id)
            .map(|v| v.url.clone())
            .ok_or_else(|| LauncherError::msg(format!("版本不存在: {id}")))
    }

    /// 递归解析继承链，合并出完整可用的版本定义
    pub async fn resolve_merged(&self, meta: &VersionMeta) -> Result<VersionMeta> {
        let mut stack: Vec<VersionMeta> = vec![meta.clone()];
        let manifest = self.fetch_manifest().await?;
        let mut guard = 0usize;
        loop {
            let top = stack.last().unwrap();
            let Some(parent_id) = top.inherits_from.clone() else {
                break;
            };
            guard += 1;
            if guard > 8 {
                return Err(LauncherError::msg("版本继承链过深"));
            }
            let url = self.find_version_url(&manifest, &parent_id).await?;
            let parent = self.fetch_version_json(&url).await?;
            stack.push(parent);
        }
        // 从父到子合并
        let mut merged = stack.pop().unwrap_or_else(|| meta.clone());
        while let Some(child) = stack.pop() {
            merge_version(&mut merged, &child);
        }
        Ok(merged)
    }

    // ---------------------------------------------------------------- 安装

    /// 安装完整版本（含继承链、assets、libraries、natives）
    pub async fn install(
        &self,
        version_id: &str,
        concurrency: usize,
    ) -> Result<InstallSummary> {
        let manifest = self.fetch_manifest().await?;
        let url = self.find_version_url(&manifest, version_id).await?;
        let raw = self.fetch_version_json(&url).await?;
        let merged = self.resolve_merged(&raw).await?;
        self.install_meta(version_id, &raw, &merged, concurrency).await
    }

    /// 安装一个已解析的版本定义（供原版/加载器实例复用）
    pub async fn install_meta(
        &self,
        version_id: &str,
        raw: &VersionMeta,
        merged: &VersionMeta,
        concurrency: usize,
    ) -> Result<InstallSummary> {
        let ver_dir = self.settings.versions_dir().join(version_id);
        std::fs::create_dir_all(&ver_dir)?;

        // 1. 写版本 json
        std::fs::write(
            ver_dir.join(format!("{version_id}.json")),
            serde_json::to_vec_pretty(raw)?,
        )?;

        // 2. client jar
        let jar_name = merged.jar.as_deref().unwrap_or(&merged.id);
        let client_task = match &merged.downloads {
            Some(d) if d.client.is_some() => {
                let a = d.client.as_ref().unwrap();
                Some(DownloadTask {
                    url: a.url.clone(),
                    dest: ver_dir.join(format!("{jar_name}.jar")),
                    sha1: Some(a.sha1.clone()),
                    size: Some(a.size),
                    label: format!("client-{version_id}"),
                })
            }
            _ => None,
        };

        // 3. asset index json
        let asset_task = match &merged.asset_index {
            Some(idx) => Some(DownloadTask {
                url: idx.url.clone(),
                dest: self
                    .settings
                    .assets_dir()
                    .join("indexes")
                    .join(format!("{}.json", idx.id)),
                sha1: Some(idx.sha1.clone()),
                size: Some(idx.size),
                label: format!("asset-index-{}", idx.id),
            }),
            None => None,
        };

        let dl = Downloader::new(concurrency);
        let mut first = Vec::new();
        if let Some(t) = client_task {
            first.push(t);
        }
        if let Some(t) = asset_task {
            first.push(t);
        }
        let r1 = dl.download_all(first).await?;

        // 4. libraries
        let lib_tasks = self.collect_library_tasks(merged)?;
        let lib_count = lib_tasks.len();
        let r2 = dl.download_all(lib_tasks).await?;

        // 5. assets objects
        let assets_count = self.download_assets(&dl, merged).await?;

        // 6. natives 解压
        let natives_extracted = self.extract_natives(merged, version_id)?;

        Ok(InstallSummary {
            version_id: version_id.to_string(),
            files_downloaded: r1.downloaded + r2.downloaded,
            files_skipped: r1.skipped + r2.skipped,
            assets_count,
            libraries_count: lib_count,
            natives_extracted,
        })
    }

    /// 收集 libraries 下载任务（含当前平台 natives classifier），已存在的跳过由下载器处理
    fn collect_library_tasks(&self, meta: &VersionMeta) -> Result<Vec<DownloadTask>> {
        let os = OsKind::current();
        let mut tasks = Vec::new();
        for lib in &meta.libraries {
            // rules 过滤
            if !crate::launcher::meta::rules_allow(&lib.rules) {
                continue;
            }
            let Some(downloads) = &lib.downloads else {
                continue;
            };
            // 主 artifact
            if let Some(a) = &downloads.artifact {
                tasks.push(DownloadTask {
                    url: a.url.clone(),
                    dest: self.settings.libraries_dir().join(&a.path),
                    sha1: Some(a.sha1.clone()),
                    size: Some(a.size),
                    label: a.path.clone(),
                });
            }
            // natives classifier（按当前平台）
            if let Some(natives) = &lib.natives {
                if let Some(classifier) = natives.get(os.natives_name()) {
                    if let Some(a) = downloads.classifiers.as_ref().and_then(|c| c.get(classifier)) {
                        tasks.push(DownloadTask {
                            url: a.url.clone(),
                            dest: self.settings.libraries_dir().join(&a.path),
                            sha1: Some(a.sha1.clone()),
                            size: Some(a.size),
                            label: a.path.clone(),
                        });
                    }
                }
            }
        }
        Ok(tasks)
    }

    async fn download_assets(
        &self,
        dl: &Downloader,
        meta: &VersionMeta,
    ) -> Result<usize> {
        let Some(idx) = &meta.asset_index else {
            return Ok(0);
        };
        let index_path = self
            .settings
            .assets_dir()
            .join("indexes")
            .join(format!("{}.json", idx.id));
        let raw = std::fs::read_to_string(&index_path)?;
        let index: crate::launcher::meta::AssetIndex = serde_json::from_str(&raw)?;

        let objects = index.objects;
        let total = objects.len();
        let mut tasks = Vec::with_capacity(total);
        for (name, obj) in objects {
            let (h1, h2) = (&obj.hash[..2], &obj.hash);
            let dest = self
                .settings
                .assets_dir()
                .join("objects")
                .join(h1)
                .join(h2);
            // 简单内存标签：用资源名截断避免任务体过大
            let label = if name.len() > 40 {
                format!("…{}", &name[name.len() - 38..])
            } else {
                name.clone()
            };
            tasks.push(DownloadTask {
                url: format!(
                    "https://resources.download.minecraft.net/{}/{}",
                    h1, h2
                ),
                dest,
                sha1: Some(obj.hash.clone()),
                size: Some(obj.size),
                label,
            });
        }
        // assets 数量大，分批以控制内存
        let mut done = 0usize;
        let chunk = 512usize;
        for chunk_tasks in tasks.chunks(chunk) {
            let r = dl.download_all(chunk_tasks.to_vec()).await?;
            done += r.downloaded + r.skipped;
            self.progress("assets", done as u64, total as u64);
        }
        Ok(done)
    }

    /// 解压 natives 库到 natives/{version_id}/（排除 extract.exclude 的包前缀）
    fn extract_natives(&self, meta: &VersionMeta, version_id: &str) -> Result<bool> {
        let os = OsKind::current();
        let natives_dir = self.settings.natives_dir().join(version_id);
        std::fs::create_dir_all(&natives_dir)?;

        let mut any = false;
        for lib in &meta.libraries {
            if !crate::launcher::meta::rules_allow(&lib.rules) {
                continue;
            }
            let Some(natives) = &lib.natives else { continue };
            let Some(classifier) = natives.get(os.natives_name()) else {
                continue;
            };
            let Some(downloads) = &lib.downloads else {
                continue;
            };
            let Some(a) = downloads
                .classifiers
                .as_ref()
                .and_then(|c| c.get(classifier))
            else {
                continue;
            };
            let jar_path = self.settings.libraries_dir().join(&a.path);
            if !jar_path.exists() {
                continue;
            }
            extract_jar_to(&jar_path, &natives_dir, lib.extract.as_ref().map(|e| &e.exclude))?;
            any = true;
        }
        Ok(any)
    }
}

/// 继承合并：parent 为基础，child 覆盖/追加
pub(crate) fn merge_version(parent: &mut VersionMeta, child: &VersionMeta) {
    if !child.main_class.is_empty() {
        parent.main_class = child.main_class.clone();
    }
    if child.assets.is_empty() {
        // keep parent
    } else {
        parent.assets = child.assets.clone();
    }
    if child.asset_index.is_some() {
        parent.asset_index = child.asset_index.clone();
    }
    if child.minecraft_arguments.is_some() {
        parent.minecraft_arguments = child.minecraft_arguments.clone();
    }
    if child.arguments.is_some() {
        parent.arguments = child.arguments.clone();
    }
    if child.jar.is_some() {
        parent.jar = child.jar.clone();
    }
    if child.downloads.is_some() {
        parent.downloads = child.downloads.clone();
    }
    if child.java_version.is_some() {
        parent.java_version = child.java_version.clone();
    }
    // libraries 追加（子库在父库之后）
    parent.libraries.extend(child.libraries.clone());
}

/// 用 zip crate 解压 jar 到目标目录
fn extract_jar_to(
    jar: &std::path::Path,
    target: &std::path::Path,
    exclude_prefixes: Option<&Vec<String>>,
) -> Result<()> {
    let file = std::fs::File::open(jar)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        // 跳过目录条目
        if entry.is_dir() {
            continue;
        }
        // exclude 规则：排除指定包前缀（META-INF 默认不抽取）
        if name.starts_with("META-INF/") || name.ends_with(".SF") || name.ends_with(".RSA") || name.ends_with(".DSA") {
            continue;
        }
        if let Some(prefixes) = exclude_prefixes {
            if prefixes.iter().any(|p| name.starts_with(&format!("{p}/"))) {
                continue;
            }
        }
        // 路径穿越防护
        let rel = PathBuf::from(&name);
        if rel.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            continue;
        }
        let out_path = target.join(rel);
        if let Some(p) = out_path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let mut out = std::fs::File::create(&out_path)?;
        std::io::copy(&mut entry, &mut out)?;
    }
    Ok(())
}
