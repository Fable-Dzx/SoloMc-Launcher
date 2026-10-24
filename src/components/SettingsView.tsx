import { useEffect, useState } from "react";
import {
  getSettings,
  microsoftLogout,
  microsoftPoll,
  microsoftStart,
  offlineLogin,
  saveSettings,
} from "../api";
import type { DeviceCode, Settings } from "../types";

interface Props {
  settings: Settings;
  onToast: (msg: string) => void;
}

export default function SettingsView({ settings, onToast }: Props) {
  const [s, setS] = useState<Settings>({ ...settings });
  const [saved, setSaved] = useState(false);
  const [device, setDevice] = useState<DeviceCode | null>(null);
  const [polling, setPolling] = useState(false);

  useEffect(() => setS({ ...settings }), [settings]);

  const save = async () => {
    try {
      await saveSettings(s);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
      onToast("设置已保存");
    } catch (e) {
      onToast(`保存失败: ${e}`);
    }
  };

  const loginOffline = async () => {
    try {
      await offlineLogin(s.offline_username);
      const fresh = await getSettings();
      setS(fresh);
      onToast(`已切换为离线账号: ${fresh.offline_username}`);
    } catch (e) {
      onToast(`离线登录失败: ${e}`);
    }
  };

  const startMs = async () => {
    try {
      const d = await microsoftStart();
      setDevice(d);
    } catch (e) {
      onToast(`微软登录启动失败: ${e}`);
    }
  };

  const pollMs = async () => {
    if (!device) return;
    setPolling(true);
    try {
      const acc = await microsoftPoll(device.device_code);
      const fresh = await getSettings();
      setS(fresh);
      setDevice(null);
      onToast(`已登录微软账号: ${acc.username}`);
    } catch (e) {
      onToast(`登录失败: ${e}`);
    } finally {
      setPolling(false);
    }
  };

  const logoutMs = async () => {
    try {
      await microsoftLogout();
      const fresh = await getSettings();
      setS(fresh);
      onToast("已退出微软账号");
    } catch (e) {
      onToast(`退出失败: ${e}`);
    }
  };

  return (
    <div>
      <div className="page-title">设置</div>
      <div className="page-sub">所有数据仅保存在本机，去中心化、无需注册</div>

      <div className="card" style={{ maxWidth: 640 }}>
        {/* 下载 */}
        <strong>下载</strong>
        <div className="divider" />
        <div className="field">
          <label>并发下载线程数：{s.download_concurrency}</label>
          <input
            type="range"
            min={1}
            max={32}
            value={s.download_concurrency}
            onChange={(e) => setS({ ...s, download_concurrency: Number(e.target.value) })}
            style={{ width: "100%" }}
          />
          <div className="row-between small muted">
            <span>1</span>
            <span>32</span>
          </div>
        </div>

        {/* 运行 */}
        <strong>运行</strong>
        <div className="divider" />
        <div className="field-row">
          <div className="field">
            <label>内存（MB）</label>
            <input
              type="number"
              min={1024}
              max={32768}
              step={256}
              value={s.memory_mb}
              onChange={(e) => setS({ ...s, memory_mb: Math.max(512, Number(e.target.value)) })}
            />
          </div>
          <div className="field">
            <label>Java 路径（留空自动探测）</label>
            <input
              placeholder="/usr/bin/java 或 C:\...\java.exe"
              value={s.java_path ?? ""}
              onChange={(e) => setS({ ...s, java_path: e.target.value || null })}
            />
          </div>
        </div>
        <div className="field">
          <label>游戏数据目录（versions/assets/libraries）</label>
          <input
            value={s.game_dir}
            onChange={(e) => setS({ ...s, game_dir: e.target.value })}
          />
        </div>

        {/* 账号 */}
        <strong>账号</strong>
        <div className="divider" />
        <div className="row-between">
          <div className="grow">
            <div className="field" style={{ marginBottom: 0 }}>
              <label>离线用户名（默认模式，无需联网）</label>
              <div className="row">
                <input
                  value={s.offline_username}
                  onChange={(e) => setS({ ...s, offline_username: e.target.value })}
                />
                <button className="btn btn-sm" onClick={() => void loginOffline()}>应用</button>
              </div>
            </div>
          </div>
        </div>

        <div style={{ marginTop: 14 }}>
          {s.microsoft_account ? (
            <div className="row-between">
              <div className="small">
                <span className="tag tag-accent">微软账号</span>
                <span>{s.microsoft_account.username}</span>
              </div>
              <button className="btn btn-sm btn-danger" onClick={() => void logoutMs()}>退出</button>
            </div>
          ) : (
            <div className="row-between">
              <div className="small muted">可选：微软正版登录（离线为主，去中心化）</div>
              <button className="btn btn-sm" onClick={() => void startMs()}>微软登录</button>
            </div>
          )}
        </div>

        {/* 模组 */}
        <div className="divider" />
        <div className="field">
          <label>微软 OAuth client_id（Azure 注册，用于设备码登录）</label>
          <input
            placeholder="Azure 应用 ID，如 00000000-0000-0000-0000-000000000000"
            value={s.microsoft_client_id}
            onChange={(e) => setS({ ...s, microsoft_client_id: e.target.value })}
          />
        </div>
        <div className="field" style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <input
            type="checkbox"
            id="autodeps"
            checked={s.auto_install_mod_deps}
            onChange={(e) => setS({ ...s, auto_install_mod_deps: e.target.checked })}
            style={{ width: "auto" }}
          />
          <label htmlFor="autodeps" style={{ marginBottom: 0 }}>安装模组时自动补全 required 依赖</label>
        </div>

        <div className="row-between" style={{ marginTop: 16 }}>
          <span className="small muted">{saved ? "✓ 已保存" : ""}</span>
          <button className="btn btn-primary" onClick={() => void save()}>保存设置</button>
        </div>
      </div>

      {/* 微软设备码模态 */}
      {device && (
        <div className="modal-mask">
          <div className="modal">
            <h3>微软账号登录</h3>
            <p className="small muted">请在浏览器中打开下方链接，输入代码完成授权：</p>
            <div className="code-block">{device.user_code}</div>
            <p className="small" style={{ textAlign: "center" }}>
              <a
                href={device.verification_uri}
                onClick={(e) => {
                  e.preventDefault();
                  void import("@tauri-apps/plugin-opener").then((m) => m.openUrl(device.verification_uri));
                }}
                style={{ color: "var(--accent)" }}
              >
                {device.verification_uri}
              </a>
            </p>
            <div className="row" style={{ marginTop: 16, justifyContent: "flex-end" }}>
              <button className="btn" onClick={() => setDevice(null)} disabled={polling}>取消</button>
              <button className="btn btn-primary" onClick={() => void pollMs()} disabled={polling}>
                {polling ? "等待授权…" : "我已授权，继续"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
