import { useCallback, useEffect, useState } from "react";
import {
  listInstalled,
  modrinthInstall,
  modrinthSearch,
  modrinthVersions,
} from "../api";
import type { InstalledVersion, ModHit, ModVersion, Settings } from "../types";

interface Props {
  settings: Settings;
  onToast: (msg: string) => void;
}

export default function ModsView({ settings, onToast }: Props) {
  const [instances, setInstances] = useState<InstalledVersion[]>([]);
  const [target, setTarget] = useState("");
  const [query, setQuery] = useState("");
  const [gameVersion, setGameVersion] = useState("");
  const [results, setResults] = useState<ModHit[]>([]);
  const [searching, setSearching] = useState(false);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [modVersions, setModVersions] = useState<ModVersion[]>([]);
  const [installing, setInstalling] = useState<string | null>(null);
  const [report, setReport] = useState<{ ok: string[]; unresolved: string[] } | null>(null);

  const refreshInstances = useCallback(() => {
    listInstalled().then((list) => {
      setInstances(list);
      if (list.length > 0) setTarget((cur) => cur || list[0].id);
    }).catch((e) => onToast(`读取实例失败: ${e}`));
  }, [onToast]);

  useEffect(() => {
    refreshInstances();
  }, [refreshInstances]);

  const search = async () => {
    if (!query.trim()) {
      onToast("请输入搜索关键词");
      return;
    }
    setSearching(true);
    setExpanded(null);
    try {
      const gv = gameVersion.trim() || undefined;
      const hits = await modrinthSearch(query.trim(), gv, undefined, 24);
      setResults(hits);
    } catch (e) {
      onToast(`搜索失败: ${e}`);
    } finally {
      setSearching(false);
    }
  };

  const toggleExpand = async (hit: ModHit) => {
    if (expanded === hit.project_id) {
      setExpanded(null);
      return;
    }
    setExpanded(hit.project_id);
    setModVersions([]);
    try {
      const gv = gameVersion.trim() || undefined;
      const vs = await modrinthVersions(hit.project_id, gv, undefined);
      setModVersions(vs);
    } catch (e) {
      onToast(`获取版本失败: ${e}`);
    }
  };

  const install = async (hit: ModHit, version: ModVersion) => {
    if (!target) {
      onToast("请先选择目标实例");
      return;
    }
    setInstalling(hit.project_id);
    setReport(null);
    try {
      const res = await modrinthInstall(
        hit.project_id,
        version.id,
        target,
        settings.download_concurrency,
        settings.auto_install_mod_deps
      );
      const ok = res.installed.map((m) => `${m.is_dependency ? "⤷ " : ""}${m.filename}`);
      setReport({ ok, unresolved: res.unresolved });
      onToast(`已安装 ${hit.title}${version.files.length > 1 ? `（${version.files.length} 个文件）` : ""}`);
    } catch (e) {
      onToast(`安装失败: ${e}`);
    } finally {
      setInstalling(null);
    }
  };

  return (
    <div>
      <div className="page-title">模组</div>
      <div className="page-sub">
        从 Modrinth 官方 API 搜索与安装 · 自动补全 required 依赖
      </div>

      {/* 搜索区 */}
      <div className="card" style={{ marginBottom: 16 }}>
        <div className="field-row">
          <div className="field" style={{ flex: 3 }}>
            <label>关键词</label>
            <input
              placeholder="模组名称 / 作者，如 sodium、jei…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && void search()}
            />
          </div>
          <div className="field" style={{ flex: 1 }}>
            <label>MC 版本（可选）</label>
            <input
              placeholder="如 1.21.4"
              value={gameVersion}
              onChange={(e) => setGameVersion(e.target.value)}
            />
          </div>
          <div className="field" style={{ flex: 1 }}>
            <label>目标实例</label>
            <select value={target} onChange={(e) => setTarget(e.target.value)}>
              {instances.length === 0 && <option value="">（无实例）</option>}
              {instances.map((i) => (
                <option key={i.id} value={i.id}>
                  {i.id}
                </option>
              ))}
            </select>
          </div>
        </div>
        <div className="row-between">
          <span className="small muted">依赖补全：{settings.auto_install_mod_deps ? "开启" : "关闭"}（可在设置中修改）</span>
          <button className="btn btn-primary" disabled={searching} onClick={() => void search()}>
            {searching ? "搜索中…" : "搜索"}
          </button>
        </div>
      </div>

      {/* 结果 */}
      {results.length === 0 && !searching && (
        <div className="empty">输入关键词搜索 Modrinth 上的模组</div>
      )}

      {results.map((hit) => (
        <div className="card item" key={hit.project_id}>
          <div className="row-between" style={{ cursor: "pointer" }} onClick={() => void toggleExpand(hit)}>
            <div className="row grow">
              {hit.icon_url ? (
                <img className="mod-icon" src={hit.icon_url} alt="" loading="lazy" />
              ) : (
                <div className="mod-icon" />
              )}
              <div className="grow">
                <div className="item-title">{hit.title}</div>
                <div className="item-sub">{hit.description}</div>
                <div className="small muted" style={{ marginTop: 4 }}>
                  ↓ {hit.downloads.toLocaleString()} 次下载
                  {hit.versions.slice(0, 4).map((v) => (
                    <span key={v} className="tag">{v}</span>
                  ))}
                </div>
              </div>
            </div>
            <span className="muted small">{expanded === hit.project_id ? "收起 ▲" : "版本 ▼"}</span>
          </div>

          {expanded === hit.project_id && (
            <div style={{ marginTop: 12, borderTop: "1px solid var(--border)", paddingTop: 12 }}>
              {modVersions.length === 0 && <div className="small muted">无匹配版本（检查 MC 版本筛选）</div>}
              {modVersions.slice(0, 8).map((v) => (
                <div className="row-between" key={v.id} style={{ padding: "7px 0", borderBottom: "1px solid var(--border)" }}>
                  <div className="grow">
                    <span className="small">{v.name || v.version_number}</span>
                    <span className="tag">{v.game_versions.join("/")}</span>
                    <span className="tag">{v.loaders.join("/")}</span>
                  </div>
                  <button
                    className="btn btn-sm btn-primary"
                    disabled={installing !== null}
                    onClick={() => void install(hit, v)}
                  >
                    {installing === hit.project_id ? "安装中…" : "安装"}
                  </button>
                </div>
              ))}
              {report && expanded === hit.project_id && installing === null && (
                <div className="small" style={{ marginTop: 10 }}>
                  <div style={{ color: "var(--accent-2)" }}>
                    ✓ {report.ok.join("、")}
                  </div>
                  {report.unresolved.length > 0 && (
                    <div style={{ color: "var(--danger)", marginTop: 4 }}>
                      ⚠ 无法自动解析依赖: {report.unresolved.join("、")}
                    </div>
                  )}
                </div>
              )}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
