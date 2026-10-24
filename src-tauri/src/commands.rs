//! Tauri 命令层：前端 invoke 的入口，负责进度事件转发与游戏进程管理

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::Result;
use crate::launcher::auth::{self, DeviceCode};
use crate::launcher::install::GameInstaller;
use crate::launcher::loaders::{self, LoaderKind};
use crate::launcher::modrinth::{self, InstalledMod, ModHit, ModVersion};
use crate::launcher::process::{self, AuthInfo};
use crate::launcher::settings::{Instance, MicrosoftAccount, Settings};

// ---------------------------------------------------------------- 全局状态

/// 运行中的游戏进程 PID（用于停止游戏）
#[derive(Default)]
pub struct GameState {
    pub pid: Mutex<Option<u32>>,
}

#[derive(Clone, Serialize)]
pub struct RemoteVersion {
    pub id: String,
    pub version_type: String,
    pub release_time: String,
}

#[derive(Clone, Serialize)]
pub struct InstalledVersion {
    pub id: String,
    pub mc_version: String,
    pub loader: Option<String>,
    pub loader_version: Option<String>,
    pub installed: bool,
}

#[derive(Clone, Serialize)]
pub struct ModInstallReportView {
    pub installed: Vec<InstalledMod>,
    pub unresolved: Vec<String>,
}

// ---------------------------------------------------------------- 设置

#[tauri::command]
pub fn get_settings() -> Settings {
    Settings::load()
}

#[tauri::command]
pub fn save_settings(settings: Settings) -> Result<()> {
    settings.save()?;
    Ok(())
}

// ---------------------------------------------------------------- 版本

/// 从 Mojang 官方拉取版本清单
#[tauri::command]
pub async fn fetch_remote_versions() -> Result<Vec<RemoteVersion>> {
    let settings = Settings::load();
    let installer = GameInstaller::new(settings);
    let manifest = installer.fetch_manifest().await?;
    Ok(manifest
        .versions
        .into_iter()
        .map(|v| RemoteVersion {
            id: v.id,
            version_type: v.version_type,
            release_time: v.release_time,
        })
        .collect())
}

/// 本地已安装实例
#[tauri::command]
pub fn list_installed() -> Vec<InstalledVersion> {
    let settings = Settings::load();
    let instances = loaders::list_instances(&settings);
    instances
        .into_iter()
        .map(|i| InstalledVersion {
            id: i.id,
            mc_version: i.mc_version,
            loader: i.loader,
            loader_version: i.loader_version,
            installed: true,
        })
        .collect()
}

/// 安装原版版本
#[tauri::command]
pub async fn install_version(
    app: AppHandle,
    version_id: String,
    concurrency: usize,
) -> Result<crate::launcher::install::InstallSummary> {
    let settings = Settings::load();
    let progress = emit_progress(app);
    let installer = GameInstaller::new(settings.clone())
        .with_progress(progress);
    let summary = installer.install(&version_id, concurrency.max(1)).await?;
    Ok(summary)
}

/// 获取某版本可用的加载器版本列表
#[tauri::command]
pub async fn loader_versions(
    loader: String,
    mc_version: String,
) -> Result<Vec<String>> {
    let kind: LoaderKind = loader.parse()?;
    loaders::loader_versions(kind, &mc_version).await
}

/// 安装加载器实例
#[tauri::command]
pub async fn install_loader(
    app: AppHandle,
    loader: String,
    mc_version: String,
    loader_version: String,
    concurrency: usize,
) -> Result<Instance> {
    let settings = Settings::load();
    let kind: LoaderKind = loader.parse()?;
    let progress = emit_progress(app);
    loaders::install_loader(
        &settings,
        kind,
        &mc_version,
        &loader_version,
        concurrency.max(1),
        Some(progress),
    )
    .await
}

// ---------------------------------------------------------------- 启动

/// 启动实例
#[tauri::command]
pub async fn launch_instance(
    app: AppHandle,
    instance_id: String,
) -> Result<i32> {
    let settings = Settings::load();
    let instances = loaders::list_instances(&settings);
    let instance = instances
        .into_iter()
        .find(|i| i.id == instance_id)
        .ok_or_else(|| crate::error::LauncherError::msg(format!("实例不存在: {instance_id}")))?;

    // 认证信息
    let auth_info = match settings.auth_mode {
        crate::launcher::settings::AuthMode::Offline => {
            let p = auth::offline_profile(&settings.offline_username);
            AuthInfo {
                username: p.username,
                uuid: p.uuid,
                access_token: "0".to_string(),
                user_type: "legacy",
            }
        }
        crate::launcher::settings::AuthMode::Microsoft => {
            let account = settings
                .microsoft_account
                .clone()
                .ok_or_else(|| crate::error::LauncherError::msg("未登录微软账号"))?;
            let (token, uuid, name, ty) = auth::auth_args(&account);
            AuthInfo {
                username: name,
                uuid,
                access_token: token,
                user_type: ty,
            }
        }
    };

    // 输出事件
    let output: process::OutputFn = {
        let app2 = app.clone();
        Arc::new(move |line| {
            let _ = app2.emit("game://output", line.clone());
        })
    };

    let state: State<GameState> = app.state();
    let mut child = process::spawn_game(&settings, &instance, &auth_info)?;
    let pid = child.id().unwrap_or(0);
    // 注册 PID 供 stop_game 强杀
    *state.pid.lock().unwrap() = Some(pid);

    // 日志转发
    let (out_task, err_task) = process::spawn_output_tasks(&mut child, Some(output))?;
    let code = child.wait().await.map_err(|e| {
        crate::error::LauncherError::msg(format!("等待游戏进程失败: {e}"))
    })?;
    let _ = out_task.await;
    let _ = err_task.await;

    // 清空 PID
    *state.pid.lock().unwrap() = None;
    let _ = app.emit("game://exit", code.code().unwrap_or(-1));
    Ok(code.code().unwrap_or(-1))
}

/// 强制停止游戏
#[tauri::command]
pub async fn stop_game(app: AppHandle) -> Result<()> {
    let state: State<GameState> = app.state();
    if let Some(pid) = state.pid.lock().unwrap().take() {
        match process::kill_pid(pid) {
            Ok(()) => log::info!("已发送停止信号 PID {pid}"),
            Err(e) => log::warn!("停止进程 {pid} 失败: {e}"),
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- 认证

#[tauri::command]
pub async fn offline_login(username: String) -> Result<String> {
    let mut settings = Settings::load();
    settings.auth_mode = crate::launcher::settings::AuthMode::Offline;
    settings.offline_username = username.trim().to_string();
    if settings.offline_username.is_empty() {
        return Err(crate::error::LauncherError::msg("用户名不能为空"));
    }
    settings.save()?;
    let p = auth::offline_profile(&settings.offline_username);
    Ok(p.uuid)
}

#[tauri::command]
pub async fn microsoft_start_device_code() -> Result<DeviceCode> {
    let settings = Settings::load();
    auth::start_device_code(&settings.microsoft_client_id).await
}

#[tauri::command]
pub async fn microsoft_poll(device_code: String) -> Result<MicrosoftAccount> {
    let settings = Settings::load();
    let account = auth::finish_device_code(&settings.microsoft_client_id, &device_code).await?;
    let mut s = Settings::load();
    s.auth_mode = crate::launcher::settings::AuthMode::Microsoft;
    s.microsoft_account = Some(account.clone());
    s.save()?;
    Ok(account)
}

#[tauri::command]
pub async fn microsoft_logout() -> Result<()> {
    let mut settings = Settings::load();
    settings.auth_mode = crate::launcher::settings::AuthMode::Offline;
    settings.microsoft_account = None;
    settings.save()?;
    Ok(())
}

// ---------------------------------------------------------------- Modrinth

#[tauri::command]
pub async fn modrinth_search(
    query: String,
    game_version: Option<String>,
    loader: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<ModHit>> {
    let client = modrinth::ModrinthClient::new();
    client
        .search(&query, game_version.as_deref(), loader.as_deref(), limit.unwrap_or(20))
        .await
}

#[tauri::command]
pub async fn modrinth_versions(
    project_id: String,
    game_version: Option<String>,
    loader: Option<String>,
) -> Result<Vec<ModVersion>> {
    let client = modrinth::ModrinthClient::new();
    client
        .project_versions(&project_id, game_version.as_deref(), loader.as_deref())
        .await
}

#[tauri::command]
pub async fn modrinth_install(
    app: AppHandle,
    project_id: String,
    version_id: String,
    instance_id: String,
    concurrency: Option<usize>,
    install_deps: Option<bool>,
) -> Result<ModInstallReportView> {
    let settings = Settings::load();
    let client = modrinth::ModrinthClient::new();
    let progress = emit_progress(app);
    let _ = progress;
    let report = client
        .install_mod(
            &settings,
            &project_id,
            &version_id,
            &instance_id,
            concurrency.unwrap_or(settings.download_concurrency),
            install_deps.unwrap_or(settings.auto_install_mod_deps),
        )
        .await?;
    Ok(ModInstallReportView {
        installed: report.installed,
        unresolved: report.unresolved,
    })
}

// ---------------------------------------------------------------- 工具

/// 构造下载进度事件回调
fn emit_progress(app: AppHandle) -> crate::launcher::install::ProgressFn {
    Arc::new(move |label: String, done: u64, total: u64| {
        let payload = serde_json::json!({
            "label": label,
            "done": done,
            "total": total,
        });
        let _ = app.emit("download://progress", payload);
    })
}
