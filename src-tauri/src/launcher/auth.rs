//! 账号认证：离线账号（去中心化默认）+ 微软 OAuth 设备码流程（可选正版）
//!
//! 微软登录完全基于官方端点：login.microsoftonline.com / user.auth.xboxlive.com /
//! xsts.auth.xboxlive.com / api.minecraftservices.com。client_id 由用户自行在
//! Azure 注册（README 有教程），不依赖任何第三方中转，符合去中心化原则。

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{LauncherError, Result};
use crate::launcher::settings::MicrosoftAccount;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OfflineProfile {
    pub uuid: String,
    pub username: String,
}

const MS_DEVICE_CODE: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const MS_TOKEN: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBL_AUTH: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_AUTH: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE: &str = "https://api.minecraftservices.com/minecraft/profile";

/// Java 兼容的离线 UUID：UUID.nameUUIDFromBytes("OfflinePlayer:" + name)（MD5 v3）
pub fn offline_uuid(username: &str) -> String {
    let mut ctx = md5::Context::new();
    ctx.consume(format!("OfflinePlayer:{username}").as_bytes());
    let mut digest = ctx.compute().0;
    digest[6] = (digest[6] & 0x0f) | 0x30; // version 3
    digest[8] = (digest[8] & 0x3f) | 0x80; // IETF variant
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        digest[0], digest[1], digest[2], digest[3],
        digest[4], digest[5],
        digest[6], digest[7],
        digest[8], digest[9],
        digest[10], digest[11], digest[12], digest[13], digest[14], digest[15],
    )
}

pub fn offline_profile(username: &str) -> OfflineProfile {
    OfflineProfile {
        uuid: offline_uuid(username),
        username: username.to_string(),
    }
}

/// 第一步：申请设备码（前端展示 user_code 与 verification_uri 给用户）
pub async fn start_device_code(client_id: &str) -> Result<DeviceCode> {
    if client_id.trim().is_empty() {
        return Err(LauncherError::msg(
            "未配置微软 client_id：请在设置中填写 Azure 注册的应用 ID（README 有步骤）",
        ));
    }
    let client = reqwest::Client::new();
    let resp = client
        .post(MS_DEVICE_CODE)
        .form(&[
            ("client_id", client_id),
            ("scope", "XboxLive.signin offline_access"),
        ])
        .send()
        .await?;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await?;
    if !status.is_success() {
        return Err(LauncherError::msg(format!(
            "设备码申请失败 ({status}): {}",
            body.get("error_description").map(|s| s.as_str().unwrap_or("")).unwrap_or("")
        )));
    }
    Ok(serde_json::from_value(body)?)
}

/// 第二步：轮询令牌并完成 XBL → XSTS → Minecraft 全链路
pub async fn finish_device_code(client_id: &str, device_code: &str) -> Result<MicrosoftAccount> {
    let client = reqwest::Client::new();
    let mut interval = 5u64;

    // 轮询 access_token / refresh_token
    let (access, refresh) = loop {
        let resp = client
            .post(MS_TOKEN)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", client_id),
                ("device_code", device_code),
            ])
            .send()
            .await?;
        let body: serde_json::Value = resp.json().await?;
        if let Some(err) = body.get("error").and_then(|e| e.as_str()) {
            match err {
                "authorization_pending" => {
                    interval = body
                        .get("interval")
                        .and_then(|i| i.as_u64())
                        .unwrap_or(5)
                        .max(5);
                    tokio::time::sleep(Duration::from_secs(interval)).await;
                    continue;
                }
                "slow_down" => {
                    tokio::time::sleep(Duration::from_secs(interval + 5)).await;
                    continue;
                }
                "authorization_declined" => {
                    return Err(LauncherError::msg("用户拒绝了授权"));
                }
                "expired_token" => {
                    return Err(LauncherError::msg("设备码已过期，请重新发起登录"));
                }
                other => {
                    return Err(LauncherError::msg(format!("设备码流程错误: {other}")));
                }
            }
        }
        match body.get("access_token").and_then(|t| t.as_str()) {
            Some(a) => {
                let refresh = body
                    .get("refresh_token")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                break (a.to_string(), refresh);
            }
            None => return Err(LauncherError::msg("未获得 access_token")),
        }
    };

    // XBL
    let xbl_token = xbl_authenticate(&client, &access).await?;
    // XSTS
    let (xsts_token, uhs) = xsts_authorize(&client, &xbl_token).await?;
    // Minecraft
    let mc_token = mc_login(&client, &uhs, &xsts_token).await?;
    // Profile
    let profile = mc_profile(&client, &mc_token).await?;

    let uuid = profile
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let username = profile
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(MicrosoftAccount {
        access_token: access,
        refresh_token: refresh,
        uuid,
        username,
    })
}

async fn xbl_authenticate(client: &reqwest::Client, ms_access_token: &str) -> Result<String> {
    let body = json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={ms_access_token}")
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT"
    });
    let resp = client
        .post(XBL_AUTH)
        .header("x-xbl-contract-version", "1")
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;
    let status = resp.status();
    let v: serde_json::Value = resp.json().await?;
    if !status.is_success() {
        return Err(LauncherError::msg(format!(
            "XBL 认证失败 ({status}): {}",
            v.get("error_description").map(|s| s.as_str().unwrap_or("")).unwrap_or("")
        )));
    }
    v.get("Token")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| LauncherError::msg("XBL 响应缺少 Token"))
}

async fn xsts_authorize(client: &reqwest::Client, xbl_token: &str) -> Result<(String, String)> {
    let body = json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl_token]
        },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT"
    });
    let resp = client
        .post(XSTS_AUTH)
        .header("x-xbl-contract-version", "1")
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;
    let status = resp.status();
    let v: serde_json::Value = resp.json().await?;
    if !status.is_success() {
        // 常见错误：XUID 缺失（未成年人账号）或账号无 Xbox 账户
        let code = v
            .get("XErr")
            .map(|x| x.as_i64().unwrap_or(0))
            .unwrap_or(0);
        return Err(LauncherError::msg(format!(
            "XSTS 授权失败 ({status}, XErr={code})：请确认账号已注册 Xbox 并允许多人游戏"
        )));
    }
    let token = v
        .get("Token")
        .and_then(|t| t.as_str())
        .ok_or_else(|| LauncherError::msg("XSTS 响应缺少 Token"))?
        .to_string();
    let uhs = v
        .pointer("/DisplayClaims/xui/0/uhs")
        .and_then(|u| u.as_str())
        .ok_or_else(|| LauncherError::msg("XSTS 响应缺少 uhs"))?
        .to_string();
    Ok((token, uhs))
}

async fn mc_login(client: &reqwest::Client, uhs: &str, xsts_token: &str) -> Result<String> {
    let identity = format!("XBL3.0 x={uhs};{xsts_token}");
    let body = json!({ "identityToken": identity });
    let resp = client
        .post(MC_LOGIN)
        .json(&body)
        .send()
        .await?;
    let status = resp.status();
    let v: serde_json::Value = resp.json().await?;
    if !status.is_success() {
        return Err(LauncherError::msg(format!(
            "Minecraft 登录失败 ({status}): {}",
            v.get("error").map(|s| s.as_str().unwrap_or("")).unwrap_or("")
        )));
    }
    v.get("access_token")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| LauncherError::msg("Minecraft 登录响应缺少 access_token"))
}

async fn mc_profile(client: &reqwest::Client, mc_token: &str) -> Result<serde_json::Value> {
    let resp = client
        .get(MC_PROFILE)
        .bearer_auth(mc_token)
        .send()
        .await?;
    let status = resp.status();
    let v: serde_json::Value = resp.json().await?;
    if !status.is_success() {
        // 该账号可能没有 Minecraft 正版
        return Err(LauncherError::msg(format!(
            "获取 Minecraft 档案失败 ({status})：请确认账号已购买 Minecraft Java 版"
        )));
    }
    Ok(v)
}

/// 将微软 token 转成启动用的认证参数（accessToken / uuid / username / userType）
pub fn auth_args(account: &MicrosoftAccount) -> (String, String, String, &'static str) {
    (
        account.access_token.clone(),
        account.uuid.clone(),
        account.username.clone(),
        "msa",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 与 Java UUID.nameUUIDFromBytes("OfflinePlayer:" + name) 兼容
    #[test]
    fn offline_uuid_matches_java() {
        assert_eq!(
            offline_uuid("Steve"),
            "5627dd98-e6be-3c21-b8a8-e92344183641"
        );
        assert_eq!(
            offline_uuid("Alex"),
            "36532b5e-c442-3dbb-a24c-c7e55d0f979a"
        );
        // 版本位 = 3，变体位 = 10xx
        let u = offline_uuid("Steve");
        assert_eq!(&u[14..15], "3");
        assert!(matches!(u.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
    }
}
