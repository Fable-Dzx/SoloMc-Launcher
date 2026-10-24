//! 可调并发数的多线程下载器
//!
//! - reqwest(rustls) + tokio Semaphore 控制并发
//! - 支持断点续传（Range 请求追加写入）
//! - 已存在且 sha1 校验通过的文件直接跳过
//! - 进度通过事件通道回调（不直接依赖 Tauri，方便单测）

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use sha1::{Digest, Sha1};
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::error::{LauncherError, Result};

/// 单个下载任务
#[derive(Debug, Clone)]
pub struct DownloadTask {
    pub url: String,
    pub dest: PathBuf,
    /// 期望 sha1 十六进制；有则校验
    pub sha1: Option<String>,
    /// 期望大小（字节）；有则用于进度估算与断点续传
    pub size: Option<u64>,
    /// 展示用标签
    pub label: String,
}

#[derive(Debug, Clone, Default)]
pub struct DownloadReport {
    pub downloaded: usize,
    pub skipped: usize,
    pub failed: usize,
}

/// 进度回调：done_bytes / total_bytes（当前任务）
pub type ProgressFn = Arc<dyn Fn(&str, u64, u64) + Send + Sync>;

pub struct Downloader {
    client: reqwest::Client,
    concurrency: usize,
    on_progress: Option<ProgressFn>,
}

impl Downloader {
    pub fn new(concurrency: usize) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(concat!("SoloMC/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            .build()
            .expect("failed to build http client");
        Self {
            client,
            concurrency: concurrency.max(1),
            on_progress: None,
        }
    }

    /// 批量下载，并发受 Semaphore 限制
    pub async fn download_all(&self, tasks: Vec<DownloadTask>) -> Result<DownloadReport> {
        let sem = Arc::new(Semaphore::new(self.concurrency));
        let mut set = JoinSet::new();
        for task in tasks {
            let sem = sem.clone();
            let client = self.client.clone();
            let on_progress = self.on_progress.clone();
            set.spawn(async move {
                let _permit = sem.acquire().await.map_err(|_| {
                    LauncherError::msg("下载并发信号量关闭")
                })?;
                download_one(&client, &task, on_progress.as_ref()).await
            });
        }

        let mut report = DownloadReport::default();
        while let Some(res) = set.join_next().await {
            match res {
                Ok(Ok(DlOutcome::Downloaded)) => report.downloaded += 1,
                Ok(Ok(DlOutcome::Skipped)) => report.skipped += 1,
                Ok(Err(e)) => {
                    report.failed += 1;
                    log::error!("下载失败: {e}");
                }
                Err(e) => {
                    report.failed += 1;
                    log::error!("下载任务 panic: {e}");
                }
            }
        }
        Ok(report)
    }
}

enum DlOutcome {
    Downloaded,
    Skipped,
}

/// 文件是否已存在且（如有 sha1）校验通过
pub fn file_valid(path: &Path, sha1: Option<&str>, size: Option<u64>) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    if let Some(size) = size {
        if meta.len() != size {
            return false;
        }
    }
    if let Some(hash) = sha1 {
        match sha1_of_file(path) {
            Ok(h) if h == hash => true,
            _ => false,
        }
    } else {
        true
    }
}

pub fn sha1_of_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    let mut h = Sha1::new();
    h.update(&bytes);
    Ok(hex(&h.finalize()))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

async fn download_one(
    client: &reqwest::Client,
    task: &DownloadTask,
    on_progress: Option<&ProgressFn>,
) -> Result<DlOutcome> {
    if let Some(parent) = task.dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // 已完整存在则跳过
    if file_valid(&task.dest, task.sha1.as_deref(), task.size) {
        log::debug!("跳过已存在: {}", task.dest.display());
        return Ok(DlOutcome::Skipped);
    }

    // 损坏的残留文件清掉
    if task.dest.exists() {
        let _ = tokio::fs::remove_file(&task.dest).await;
    }

    // 断点续传：临时文件续写
    let tmp = task.dest.with_extension("part");
    let resume_from = tokio::fs::metadata(&tmp).await.map(|m| m.len()).unwrap_or(0);

    let mut req = client.get(&task.url);
    if resume_from > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={resume_from}-"));
    }
    let resp = req.send().await?;
    let status = resp.status();
    if !status.is_success() && status != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(LauncherError::HttpStatus(status));
    }

    let mut file = if resume_from > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(&tmp)
            .await?
    } else {
        tokio::fs::File::create(&tmp).await?
    };

    let total = task
        .size
        .or_else(|| resp.content_length())
        .unwrap_or(0);
    let mut done = resume_from;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        done += chunk.len() as u64;
        if let Some(f) = on_progress {
            f(&task.label, done, total.max(done));
        }
    }
    file.flush().await?;
    drop(file);

    // 校验并落位
    if let Some(hash) = &task.sha1 {
        let actual = sha1_of_file(&tmp)?;
        if &actual != hash {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(LauncherError::msg(format!(
                "sha1 校验失败: {} (期望 {} 实际 {})",
                task.label, hash, actual
            )));
        }
    }
    tokio::fs::rename(&tmp, &task.dest).await?;
    Ok(DlOutcome::Downloaded)
}
