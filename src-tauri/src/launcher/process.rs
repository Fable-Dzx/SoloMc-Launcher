//! 游戏进程启动：构建 JVM 参数、启动子进程、逐行转发日志
//!
//! 版本隔离：每个实例以自身目录为 --gameDir（saves/mods/config 互不干扰）。
//! 本地优先：启动时只读本地 versions 目录中的 json，不依赖网络。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, BufReader};

use crate::error::{LauncherError, Result};
use crate::launcher::install::merge_version;
use crate::launcher::meta::VersionMeta;
use crate::launcher::settings::{Instance, Settings};

/// 认证信息（由命令层根据设置组装）
#[derive(Debug, Clone)]
pub struct AuthInfo {
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    pub user_type: &'static str,
}

/// 输出回调：逐行游戏日志
pub type OutputFn = Arc<dyn Fn(String) + Send + Sync>;

/// 从本地 versions 目录读取并合并继承链
pub fn load_local_version(settings: &Settings, version_id: &str) -> Result<VersionMeta> {
    let versions = settings.versions_dir();
    let mut stack: Vec<VersionMeta> = Vec::new();
    let mut cur = version_id.to_string();
    let mut guard = 0usize;
    loop {
        guard += 1;
        if guard > 8 {
            return Err(LauncherError::msg("版本继承链过深"));
        }
        let p = versions.join(&cur).join(format!("{cur}.json"));
        let text = std::fs::read_to_string(&p)
            .map_err(|_| LauncherError::msg(format!("本地版本缺失: {}（请先安装）", p.display())))?;
        let meta: VersionMeta = serde_json::from_str(&text)?;
        match meta.inherits_from.clone() {
            Some(parent) => {
                stack.push(meta);
                cur = parent;
            }
            None => {
                stack.push(meta);
                break;
            }
        }
    }
    let mut merged = stack.pop().ok_or_else(|| LauncherError::msg("空版本链"))?;
    while let Some(child) = stack.pop() {
        merge_version(&mut merged, &child);
    }
    Ok(merged)
}

/// 探测 Java：配置路径 > JAVA_HOME > PATH
pub fn find_java(configured: Option<&Path>) -> Result<PathBuf> {
    let exe = if cfg!(windows) { "java.exe" } else { "java" };
    if let Some(p) = configured {
        let cand = if p.file_name().map(|f| f == exe).unwrap_or(false) {
            p.to_path_buf()
        } else {
            p.join(exe)
        };
        if cand.exists() {
            return Ok(cand);
        }
    }
    if let Ok(home) = std::env::var("JAVA_HOME") {
        let cand = Path::new(&home).join("bin").join(exe);
        if cand.exists() {
            return Ok(cand);
        }
    }
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let cand = dir.join(exe);
            if cand.exists() {
                return Ok(cand);
            }
        }
    }
    Err(LauncherError::msg(
        "未找到 Java：请在设置中指定 java 路径，或安装 Java 17/21 并加入 PATH",
    ))
}

/// 组装 JVM + 游戏参数
pub fn build_args(
    settings: &Settings,
    instance: &Instance,
    merged: &VersionMeta,
    auth: &AuthInfo,
) -> Result<Vec<String>> {
    let java = find_java(settings.java_path.as_deref())?;
    let instance_dir = instance.dir(settings);
    let assets_dir = settings.assets_dir();
    let natives_dir = settings.natives_dir().join(&instance.id);
    let libs_dir = settings.libraries_dir();

    // classpath：libraries + client jar
    let mut classpath: Vec<PathBuf> = Vec::new();
    for lib in &merged.libraries {
        if !crate::launcher::meta::rules_allow(&lib.rules) {
            continue;
        }
        if let Some(d) = &lib.downloads {
            if let Some(a) = &d.artifact {
                let p = libs_dir.join(&a.path);
                if p.exists() {
                    classpath.push(p);
                }
            }
        }
    }
    let jar_name = merged.jar.as_deref().unwrap_or(&merged.id);
    let client_jar = instance_dir.join(format!("{jar_name}.jar"));
    if client_jar.exists() {
        classpath.push(client_jar);
    }
    if classpath.is_empty() {
        return Err(LauncherError::msg("classpath 为空，版本文件不完整"));
    }

    let sep = if cfg!(windows) { ";" } else { ":" };
    let cp = classpath
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(sep);

    let asset_index = merged.asset_index.as_ref().map(|i| i.id.clone()).unwrap_or_default();
    let version_type = "release";

    let mut placeholders: HashMap<&str, String> = HashMap::new();
    placeholders.insert("auth_player_name", auth.username.clone());
    placeholders.insert("auth_session", auth.access_token.clone());
    placeholders.insert("auth_access_token", auth.access_token.clone());
    placeholders.insert("auth_uuid", auth.uuid.clone());
    placeholders.insert("user_type", auth.user_type.to_string());
    placeholders.insert("version_name", instance.version_id.clone());
    placeholders.insert("version_type", version_type.to_string());
    placeholders.insert("game_directory", instance_dir.to_string_lossy().to_string());
    placeholders.insert("assets_root", assets_dir.to_string_lossy().to_string());
    placeholders.insert("assets_index_name", asset_index);
    placeholders.insert(
        "game_assets",
        assets_dir.join("virtual").join(merged.assets.clone()).to_string_lossy().to_string(),
    );
    placeholders.insert("natives_directory", natives_dir.to_string_lossy().to_string());
    placeholders.insert("launcher_name", "SoloMC".to_string());
    placeholders.insert("launcher_version", env!("CARGO_PKG_VERSION").to_string());
    placeholders.insert("classpath", cp.clone());
    placeholders.insert("classpath_separator", sep.to_string());
    placeholders.insert("resolution_width", "854".to_string());
    placeholders.insert("resolution_height", "480".to_string());

    let mut args: Vec<String> = Vec::new();
    args.push(java.to_string_lossy().to_string());
    args.push(format!("-Xmx{}M", settings.memory_mb.max(512)));
    args.push(format!("-Xms{}M", (settings.memory_mb / 4).max(256)));
    args.push(format!("-Djava.library.path={}", natives_dir.to_string_lossy()));

    // JVM 参数（现代 arguments.jvm 或旧版 minecraftArguments 里的 JVM 段）
    if let Some(arguments) = &merged.arguments {
        args.extend(VersionMeta::expand_arguments(&arguments.jvm, &placeholders));
    }
    args.push("-cp".to_string());
    args.push(cp);

    // mainClass 可能含空格（Forge 的 BootstrapLauncher）
    let main = merged.main_class.trim();
    if main.is_empty() {
        return Err(LauncherError::msg("版本定义缺少 mainClass"));
    }
    args.extend(main.split_whitespace().map(|s| s.to_string()));

    // 游戏参数
    if let Some(arguments) = &merged.arguments {
        args.extend(VersionMeta::expand_arguments(&arguments.game, &placeholders));
    } else if let Some(mc_args) = &merged.minecraft_arguments {
        // 旧版：整串替换占位符后按空白切分（保持简单，引号内空格不做特殊处理）
        let mut s = mc_args.clone();
        for (k, v) in &placeholders {
            s = s.replace(&format!("${{{k}}}"), v);
        }
        args.extend(s.split_whitespace().map(|t| t.to_string()));
    }

    // 兜底注入（旧版参数未覆盖时）
    let needed = [
        "--gameDir",
        "--assetsDir",
        "--assetIndex",
        "--uuid",
        "--accessToken",
        "--username",
        "--userType",
        "--version",
    ];
    for n in needed {
        if !args.iter().any(|a| a == n) {
            let v = match n {
                "--gameDir" => placeholders["game_directory"].clone(),
                "--assetsDir" => placeholders["assets_root"].clone(),
                "--assetIndex" => placeholders["assets_index_name"].clone(),
                "--uuid" => placeholders["auth_uuid"].clone(),
                "--accessToken" => placeholders["auth_access_token"].clone(),
                "--username" => placeholders["auth_player_name"].clone(),
                "--userType" => placeholders["user_type"].clone(),
                "--version" => placeholders["version_name"].clone(),
                _ => unreachable!(),
            };
            args.push(n.to_string());
            args.push(v);
        }
    }

    Ok(args)
}

/// 启动游戏子进程（stdout/stderr 保持管道），返回句柄
pub fn spawn_game(
    settings: &Settings,
    instance: &Instance,
    auth: &AuthInfo,
) -> Result<tokio::process::Child> {
    let merged = load_local_version(settings, &instance.id)?;
    let args = build_args(settings, instance, &merged, auth)?;

    // 日志里不得泄露 access_token
    let safe_args: Vec<String> = args
        .iter()
        .map(|a| {
            if !auth.access_token.is_empty() && auth.access_token != "0" && a.contains(&auth.access_token) {
                "***".to_string()
            } else {
                a.clone()
            }
        })
        .collect();
    log::info!("启动命令: {}", safe_args.join(" "));

    let mut cmd = tokio::process::Command::new(&args[0]);
    cmd.args(&args[1..])
        .current_dir(instance.dir(settings))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    cmd.spawn()
        .map_err(|e| LauncherError::msg(format!("启动游戏失败: {e}")))
}

/// 取出 stdout/stderr 并启动两个日志转发 task
pub fn spawn_output_tasks(
    child: &mut tokio::process::Child,
    on_output: Option<OutputFn>,
) -> Result<(tokio::task::JoinHandle<()>, tokio::task::JoinHandle<()>)> {
    let stdout = child.stdout.take().ok_or_else(|| LauncherError::msg("无法读取 stdout"))?;
    let stderr = child.stderr.take().ok_or_else(|| LauncherError::msg("无法读取 stderr"))?;

    let out_cb = on_output.clone();
    let out_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            if let Some(f) = &out_cb {
                f(line);
            }
        }
    });
    let err_cb = on_output;
    let err_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            if let Some(f) = &err_cb {
                f(line);
            }
        }
    });
    Ok((out_task, err_task))
}

/// 跨平台按 PID 强杀进程
pub fn kill_pid(pid: u32) -> Result<()> {
    #[cfg(unix)]
    let status = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status();
    #[cfg(windows)]
    let status = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .status();
    status?;
    Ok(())
}
