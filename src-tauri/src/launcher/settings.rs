//! 本地设置持久化：~/.config/solo-mc/settings.json

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::LauncherError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    Offline,
    Microsoft,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicrosoftAccount {
    /// 微软 OAuth access_token（用于获取 XBL 票据，可刷新）
    pub access_token: String,
    /// 用于刷新 access_token
    pub refresh_token: String,
    /// Minecraft profile
    pub uuid: String,
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// 游戏数据根目录（含 versions/assets/libraries/natives）
    pub game_dir: PathBuf,
    /// Java 可执行文件路径；为空时自动探测 PATH / JAVA_HOME
    pub java_path: Option<PathBuf>,
    /// 分配内存（MB）
    pub memory_mb: u64,
    /// 下载并发数
    pub download_concurrency: usize,
    /// 账号模式
    pub auth_mode: AuthMode,
    /// 离线用户名
    pub offline_username: String,
    /// 微软账号（登录后填充）
    pub microsoft_account: Option<MicrosoftAccount>,
    /// 微软 OAuth client_id（设备码流程必需，用户可自行在 Azure 注册）
    pub microsoft_client_id: String,
    /// 是否自动补全模组依赖
    pub auto_install_mod_deps: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            game_dir: default_game_dir(),
            java_path: None,
            memory_mb: 2048,
            download_concurrency: 8,
            auth_mode: AuthMode::Offline,
            offline_username: "Steve".to_string(),
            microsoft_account: None,
            microsoft_client_id: String::new(),
            auto_install_mod_deps: true,
        }
    }
}

pub fn default_game_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("SoloMC")
}

impl Settings {
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("solo-mc")
            .join("settings.json")
    }

    pub fn load() -> Self {
        let p = Self::path();
        match std::fs::read_to_string(&p) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|_| {
                log::warn!("settings.json 解析失败，使用默认配置");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), LauncherError> {
        let p = Self::path();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let s = serde_json::to_string_pretty(self)?;
        std::fs::write(&p, s)?;
        Ok(())
    }

    /// 运行时目录：libraries / assets / versions / natives
    pub fn libraries_dir(&self) -> PathBuf {
        self.game_dir.join("libraries")
    }
    pub fn assets_dir(&self) -> PathBuf {
        self.game_dir.join("assets")
    }
    pub fn versions_dir(&self) -> PathBuf {
        self.game_dir.join("versions")
    }
    pub fn natives_dir(&self) -> PathBuf {
        self.game_dir.join("natives")
    }
}

/// 一个已安装的版本实例（版本隔离：每个版本/加载器组合有独立游戏目录）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    /// 实例 id（目录名），如 "1.20.1-fabric"
    pub id: String,
    /// 版本 json id
    pub version_id: String,
    /// 所属 Minecraft 版本，如 "1.20.1"
    pub mc_version: String,
    /// 加载器信息
    pub loader: Option<String>,
    /// 加载器版本
    pub loader_version: Option<String>,
}

impl Instance {
    /// 实例游戏目录（版本隔离：saves/mods/config 都在各自目录内）
    pub fn dir(&self, settings: &Settings) -> PathBuf {
        settings.versions_dir().join(&self.id)
    }
}
