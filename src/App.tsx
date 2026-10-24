import { useEffect, useState } from "react";
import GamesView from "./components/GamesView";
import ModsView from "./components/ModsView";
import SettingsView from "./components/SettingsView";
import { getSettings, onDownloadProgress, onGameExit, onGameOutput, stopGame } from "./api";
import type { DownloadProgress, Settings } from "./types";

type Page = "games" | "mods" | "settings";

const NAV: { key: Page; label: string; icon: string }[] = [
  { key: "games", label: "游戏", icon: "▣" },
  { key: "mods", label: "模组", icon: "◈" },
  { key: "settings", label: "设置", icon: "⚙" },
];

export default function App() {
  const [page, setPage] = useState<Page>("games");
  const [settings, setSettings] = useState<Settings | null>(null);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [gameRunning, setGameRunning] = useState(false);
  const [logLines, setLogLines] = useState<string[]>([]);
  const [toast, setToast] = useState<string | null>(null);

  // 加载设置
  useEffect(() => {
    getSettings().then(setSettings).catch((e) => setToast(`读取设置失败: ${e}`));
  }, []);

  // 下载进度事件
  useEffect(() => {
    let un: (() => void) | undefined;
    onDownloadProgress((p) => setProgress(p)).then((f) => (un = f));
    return () => un?.();
  }, []);

  // 游戏输出 / 退出事件
  useEffect(() => {
    let un1: (() => void) | undefined;
    let un2: (() => void) | undefined;
    onGameOutput((line) => setLogLines((prev) => [...prev.slice(-400), line])).then(
      (f) => (un1 = f)
    );
    onGameExit((code) => {
      setGameRunning(false);
      setLogLines((prev) => [...prev, `--- 游戏退出，代码 ${code} ---`]);
    }).then((f) => (un2 = f));
    return () => {
      un1?.();
      un2?.();
    };
  }, []);

  // 启动时清空日志
  const handleLaunchStart = () => {
    setLogLines([]);
    setGameRunning(true);
  };

  const showToast = (msg: string) => {
    setToast(msg);
    setTimeout(() => setToast(null), 3000);
  };

  if (!settings) {
    return <div className="empty">加载中…</div>;
  }

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="logo">
          <span className="logo-badge">S</span>
          SoloMC
        </div>
        {NAV.map((n) => (
          <div
            key={n.key}
            className={`nav-item ${page === n.key ? "active" : ""}`}
            onClick={() => setPage(n.key)}
          >
            <span className="nav-icon">{n.icon}</span>
            {n.label}
          </div>
        ))}
        <div className="sidebar-footer">
          <div className={gameRunning ? "" : "muted"}>
            {gameRunning ? "● 游戏运行中" : "○ 未运行"}
          </div>
          <div>SoloMC v0.1.0 · 去中心化启动器</div>
        </div>
      </aside>

      <main className="main">
        {page === "games" && (
          <GamesView
            settings={settings}
            onLaunchStart={handleLaunchStart}
            gameRunning={gameRunning}
            progress={progress}
            onToast={showToast}
          />
        )}
        {page === "mods" && (
          <ModsView settings={settings} onToast={showToast} />
        )}
        {page === "settings" && (
          <SettingsView settings={settings} onToast={showToast} />
        )}
      </main>

      {toast && <div className="modal-mask" style={{ pointerEvents: "none", background: "transparent", placeItems: "start center", paddingTop: 60 }}>
        <div className="card" style={{ background: "var(--bg-3)", borderColor: "var(--accent)" }}>{toast}</div>
      </div>}

      {gameRunning && (
        <div className="modal-mask" style={{ placeItems: "end center", background: "transparent", paddingBottom: 24, pointerEvents: "none" }}>
          <div style={{ width: "70%", pointerEvents: "auto" }}>
            <div className="card" style={{ boxShadow: "var(--shadow)" }}>
              <div className="row-between">
                <strong>游戏控制台</strong>
                <button className="btn btn-danger btn-sm" onClick={() => void stopGame()}>
                  停止游戏
                </button>
              </div>
              <div className="console" style={{ marginTop: 10 }}>{logLines.join("\n")}</div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
