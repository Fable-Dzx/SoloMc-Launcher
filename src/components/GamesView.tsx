import { useCallback, useEffect, useMemo, useState } from "react";
import {
  fetchRemoteVersions,
  installLoader,
  installVersion,
  launchInstance,
  listInstalled,
  loaderVersions,
} from "../api";
import type { DownloadProgress, InstalledVersion, RemoteVersion, Settings } from "../types";

type LoaderKey = "none" | "fabric" | "forge" | "neoforge";

const LOADERS: { key: LoaderKey; label: string }[] = [
  { key: "none", label: "原版（无加载器）" },
  { key: "fabric", label: "Fabric" },
  { key: "forge", label: "Forge" },
  { key: "neoforge", label: "NeoForge" },
];

interface Props {
  settings: Settings;
  gameRunning: boolean;
  progress: DownloadProgress | null;
  onLaunchStart: () => void;
  onToast: (msg: string) => void;
}

export default function GamesView({ settings, gameRunning, progress, onLaunchStart, onToast }: Props) {
  const [versions, setVersions] = useState<RemoteVersion[]>([]);
  const [installed, setInstalled] = useState<InstalledVersion[]>([]);
  const [filter, setFilter] = useState("");
  const [selected, setSelected] = useState<RemoteVersion | null>(null);
  const [loader, setLoader] = useState<LoaderKey>("none");
  const [loaderVers, setLoaderVers] = useState<string[]>([]);
  const [loaderChoice, setLoaderChoice] = useState("");
  const [busy, setBusy] = useState(false);
  const [installing, setInstalling] = useState<string | null>(null);

  const refreshInstalled = useCallback(() => {
    listInstalled().then(setInstalled).catch((e) => onToast(`读取实例失败: ${e}`));
  }, [onToast]);

  useEffect(() => {
    fetchRemoteVersions()
      .then((vs) => {
        setVersions(vs);
        const release = vs.find((v) => v.version_type === "release");
        if (release) setSelected(release);
      })
      .catch((e) => onToast(`获取版本清单失败: ${e}`));
    refreshInstalled();
  }, [refreshInstalled, onToast]);

  // 加载器版本列表
  useEffect(() => {
    if (loader === "none" || !selected) {
      setLoaderVers([]);
      setLoaderChoice("");
      return;
    }
    let cancelled = false;
    setBusy(true);
    loaderVersions(loader, selected.id)
      .then((vs) => {
        if (cancelled) return;
        setLoaderVers(vs);
        setLoaderChoice(vs[0] ?? "");
      })
      .catch((e) => {
        if (!cancelled) onToast(`获取加载器版本失败: ${e}`);
      })
      .finally(() => !cancelled && setBusy(false));
    return () => {
      cancelled = true;
    };
  }, [loader, selected, onToast]);

  const filtered = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return versions;
    return versions.filter((v) => v.id.toLowerCase().includes(q));
  }, [versions, filter]);

  const install = async () => {
    if (!selected) return;
    const mc = selected.id;
    setInstalling(mc);
    try {
      if (loader === "none") {
        await installVersion(mc, settings.download_concurrency);
        onToast(`原版 ${mc} 安装完成`);
      } else {
        if (!loaderChoice) {
          onToast("请先选择加载器版本");
          return;
        }
        await installLoader(loader, mc, loaderChoice, settings.download_concurrency);
        onToast(`${mc} + ${loader} ${loaderChoice} 安装完成`);
      }
      refreshInstalled();
    } catch (e) {
      onToast(`安装失败: ${e}`);
    } finally {
      setInstalling(null);
    }
  };

  const launch = async (id: string) => {
    if (gameRunning) {
      onToast("游戏已在运行");
      return;
    }
    onLaunchStart();
    try {
      await launchInstance(id);
    } catch (e) {
      onToast(`启动失败: ${e}`);
    }
  };

  const pct = progress && progress.total > 0 ? Math.min(100, (progress.done / progress.total) * 100) : 0;

  return (
    <div>
      <div className="page-title">游戏</div>
      <div className="page-sub">
        从 Mojang 官方获取版本 · 版本隔离 · 可选 Fabric / Forge / NeoForge
      </div>

      <div className="grid" style={{ gridTemplateColumns: "1fr 1fr" }}>
        {/* 左：已安装实例 */}
        <div>
          <div className="row-between" style={{ marginBottom: 12 }}>
            <strong>已安装实例</strong>
            <span className="small muted">{installed.length} 个</span>
          </div>
          {installed.length === 0 && (
            <div className="empty">尚未安装任何版本，请在右侧选择并安装</div>
          )}
          {installed.map((inst) => (
            <div className="card item" key={inst.id}>
              <div className="row-between">
                <div className="grow">
                  <div className="item-title">{inst.id}</div>
                  <div className="item-sub">
                    {inst.loader ? (
                      <>
                        <span className="tag tag-accent">{inst.loader}</span>
                        <span className="tag">{inst.loader_version}</span>
                      </>
                    ) : (
                      <span className="tag">原版</span>
                    )}
                    <span className="tag">MC {inst.mc_version}</span>
                  </div>
                </div>
                <button
                  className="btn btn-success"
                  disabled={gameRunning}
                  onClick={() => void launch(inst.id)}
                >
                  ▶ 启动
                </button>
              </div>
            </div>
          ))}
        </div>

        {/* 右：安装新版本 */}
        <div>
          <strong style={{ display: "block", marginBottom: 12 }}>安装新版本</strong>
          <div className="field">
            <label>Minecraft 版本（来自 Mojang 官方清单）</label>
            <input
              placeholder="搜索版本，如 1.21.4 / snapshot"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
            />
          </div>

          <div style={{ maxHeight: 220, overflowY: "auto", marginBottom: 14 }}>
            {filtered.slice(0, 120).map((v) => (
              <div
                key={v.id}
                className={`card item version-item ${selected?.id === v.id ? "selected" : ""}`}
                onClick={() => setSelected(v)}
              >
                <div className="row-between">
                  <div className="item-title">{v.id}</div>
                  <span className="tag">{v.version_type}</span>
                </div>
              </div>
            ))}
          </div>

          <div className="field">
            <label>加载器</label>
            <select value={loader} onChange={(e) => setLoader(e.target.value as LoaderKey)}>
              {LOADERS.map((l) => (
                <option key={l.key} value={l.key}>
                  {l.label}
                </option>
              ))}
            </select>
          </div>

          {loader !== "none" && (
            <div className="field">
              <label>加载器版本 {busy && "(加载中…)"}</label>
              <select
                value={loaderChoice}
                onChange={(e) => setLoaderChoice(e.target.value)}
                disabled={loaderVers.length === 0}
              >
                {loaderVers.length === 0 && <option>无可用版本</option>}
                {loaderVers.map((v) => (
                  <option key={v} value={v}>
                    {v}
                  </option>
                ))}
              </select>
            </div>
          )}

          <div className="row-between" style={{ marginTop: 6 }}>
            <div className="small muted">
              {selected ? `已选: ${selected.id}` : "未选择版本"}
            </div>
            <button className="btn btn-primary" disabled={!selected || busy || installing !== null} onClick={() => void install()}>
              {installing !== null ? "安装中…" : "安装"}
            </button>
          </div>

          {progress && (
            <div style={{ marginTop: 14 }}>
              <div className="row-between small">
                <span className="muted grow" style={{ textOverflow: "ellipsis", overflow: "hidden" }}>
                  {progress.label}
                </span>
                <span>
                  {Math.round(pct)}%
                </span>
              </div>
              <div className="progress">
                <div style={{ width: `${pct}%` }} />
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
