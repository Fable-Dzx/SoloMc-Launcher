//! Mojang 版本元数据：版本清单 / version.json / assets index 解析
//!
//! 所有数据来自官方公开接口，不依赖任何第三方中心服务：
//! - 版本清单: https://launchermeta.mojang.com/mc/game/version_manifest_v2.json
//! - 版本定义: 清单中每条记录指向的 version.json
//! - 资源索引: version.json 中的 assetIndex

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------- 版本清单

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionManifest {
    pub latest: Latest,
    pub versions: Vec<ManifestVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestVersion {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: String,
    pub url: String,
    pub time: String,
    #[serde(rename = "releaseTime")]
    pub release_time: String,
}

// ---------------------------------------------------------------- version.json

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionMeta {
    pub id: String,
    pub assets: String,
    #[serde(rename = "assetIndex")]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(rename = "mainClass")]
    pub main_class: String,
    #[serde(rename = "minecraftArguments")]
    pub minecraft_arguments: Option<String>,
    pub arguments: Option<Arguments>,
    pub libraries: Vec<Library>,
    #[serde(rename = "inheritsFrom")]
    pub inherits_from: Option<String>,
    pub jar: Option<String>,
    pub downloads: Option<Downloads>,
    #[serde(rename = "javaVersion")]
    pub java_version: Option<JavaVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
    #[serde(rename = "totalSize")]
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaVersion {
    #[serde(rename = "majorVersion")]
    pub major_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Downloads {
    pub client: Option<Artifact>,
    pub client_mappings: Option<Artifact>,
    pub server: Option<Artifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Arguments {
    pub game: Vec<ArgValue>,
    pub jvm: Vec<ArgValue>,
}

/// 参数项可能是普通字符串，也可能是带规则的对象
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgValue {
    Plain(String),
    Rule {
        rules: Vec<Rule>,
        value: RuleValue,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RuleValue {
    Single(String),
    Multi(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub action: String,
    pub os: Option<OsRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub name: String,
    pub downloads: Option<LibraryDownloads>,
    pub rules: Option<Vec<Rule>>,
    pub natives: Option<HashMap<String, String>>,
    pub extract: Option<Extract>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryDownloads {
    pub artifact: Option<Artifact>,
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub path: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Extract {
    pub exclude: Vec<String>,
}

// ---------------------------------------------------------------- assets index

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIndex {
    pub objects: HashMap<String, AssetObject>,
    #[serde(rename = "map_to_resources")]
    pub map_to_resources: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

// ---------------------------------------------------------------- 客户端能力

/// 操作系统标识（用于 rules 匹配）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsKind {
    Windows,
    MacOs,
    Linux,
    Other,
}

impl OsKind {
    pub fn current() -> Self {
        match std::env::consts::OS {
            "windows" => OsKind::Windows,
            "macos" => OsKind::MacOs,
            "linux" => OsKind::Linux,
            _ => OsKind::Other,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            OsKind::Windows => "windows",
            OsKind::MacOs => "osx",
            OsKind::Linux => "linux",
            OsKind::Other => "unknown",
        }
    }

    /// natives classifier 名：windows / osx / linux
    pub fn natives_name(&self) -> &'static str {
        match self {
            OsKind::Windows => "natives-windows",
            OsKind::MacOs => "natives-osx",
            OsKind::Linux => "natives-linux",
            OsKind::Other => "natives-unknown",
        }
    }
}

impl From<String> for OsKind {
    fn from(s: String) -> Self {
        match s.as_str() {
            "windows" => OsKind::Windows,
            "osx" => OsKind::MacOs,
            "linux" => OsKind::Linux,
            _ => OsKind::Other,
        }
    }
}

/// 判断一组 rules 是否适用于当前平台（无 rules 时视为适用）
pub fn rules_allow(rules: &Option<Vec<Rule>>) -> bool {
    let Some(rules) = rules else {
        return true;
    };
    let os = OsKind::current();
    let arch = std::env::consts::ARCH; // x86_64 / aarch64 ...
    let mut allowed = false;
    for r in rules {
        let matched = match &r.os {
            None => true,
            Some(os_rule) => {
                let os_ok = os_rule
                    .name
                    .as_ref()
                    .map(|n| n == os.as_str())
                    .unwrap_or(true);
                let arch_ok = os_rule
                    .arch
                    .as_ref()
                    .map(|a| arch_matches(a, arch))
                    .unwrap_or(true);
                os_ok && arch_ok
            }
        };
        if matched {
            allowed = r.action == "allow";
        }
    }
    allowed
}

fn arch_matches(rule_arch: &str, current: &str) -> bool {
    // Mojang 用 x86 / arm 粗粒度，x86_64 命中 x86
    match rule_arch {
        "x86" => current == "x86" || current == "x86_64",
        "arm" => current == "aarch64" || current == "arm",
        _ => current == rule_arch,
    }
}

impl VersionMeta {
    /// 取某个参数集合展开后的字符串列表（含 rules 求值）
    pub fn expand_arguments(
        args: &[ArgValue],
        placeholders: &HashMap<&str, String>,
    ) -> Vec<String> {
        let mut out = Vec::new();
        for a in args {
            match a {
                ArgValue::Plain(s) => out.push(s.clone()),
                ArgValue::Rule { rules, value } => {
                    if rules_allow(&Some(rules.clone())) {
                        match value {
                            RuleValue::Single(s) => out.push(s.clone()),
                            RuleValue::Multi(v) => out.extend(v.iter().cloned()),
                        }
                    }
                }
            }
        }
        // 替换占位符 ${name}
        out.into_iter()
            .map(|s| {
                let mut s = s;
                for (k, v) in placeholders {
                    s = s.replace(&format!("${{{k}}}"), v);
                }
                s
            })
            .collect()
    }
}
