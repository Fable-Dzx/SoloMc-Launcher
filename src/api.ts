import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  DeviceCode,
  DownloadProgress,
  InstallSummary,
  InstalledMod,
  InstalledVersion,
  MicrosoftAccount,
  ModHit,
  ModInstallReportView,
  ModVersion,
  RemoteVersion,
  Settings,
} from "./types";

// ---------------------------------------------------------------- 设置

export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (s: Settings) => invoke<void>("save_settings", { settings: s });

// ---------------------------------------------------------------- 版本

export const fetchRemoteVersions = () => invoke<RemoteVersion[]>("fetch_remote_versions");
export const listInstalled = () => invoke<InstalledVersion[]>("list_installed");
export const installVersion = (versionId: string, concurrency: number) =>
  invoke<InstallSummary>("install_version", { versionId, concurrency });
export const loaderVersions = (loader: string, mcVersion: string) =>
  invoke<string[]>("loader_versions", { loader, mcVersion });
export const installLoader = (
  loader: string,
  mcVersion: string,
  loaderVersion: string,
  concurrency: number
) =>
  invoke<InstalledVersion>("install_loader", {
    loader,
    mcVersion,
    loaderVersion,
    concurrency,
  });

// ---------------------------------------------------------------- 启动

export const launchInstance = (instanceId: string) =>
  invoke<number>("launch_instance", { instanceId });
export const stopGame = () => invoke<void>("stop_game");

// ---------------------------------------------------------------- 认证

export const offlineLogin = (username: string) => invoke<string>("offline_login", { username });
export const microsoftStart = () => invoke<DeviceCode>("microsoft_start_device_code");
export const microsoftPoll = (deviceCode: string) =>
  invoke<MicrosoftAccount>("microsoft_poll", { deviceCode });
export const microsoftLogout = () => invoke<void>("microsoft_logout");

// ---------------------------------------------------------------- Modrinth

export const modrinthSearch = (
  query: string,
  gameVersion?: string,
  loader?: string,
  limit?: number
) =>
  invoke<ModHit[]>("modrinth_search", {
    query,
    gameVersion: gameVersion || null,
    loader: loader || null,
    limit: limit ?? 20,
  });

export const modrinthVersions = (projectId: string, gameVersion?: string, loader?: string) =>
  invoke<ModVersion[]>("modrinth_versions", {
    projectId,
    gameVersion: gameVersion || null,
    loader: loader || null,
  });

export const modrinthInstall = (
  projectId: string,
  versionId: string,
  instanceId: string,
  concurrency: number,
  installDeps: boolean
) =>
  invoke<ModInstallReportView>("modrinth_install", {
    projectId,
    versionId,
    instanceId,
    concurrency,
    installDeps,
  });

// ---------------------------------------------------------------- 事件

export function onDownloadProgress(cb: (p: DownloadProgress) => void): Promise<UnlistenFn> {
  return listen<DownloadProgress>("download://progress", (e) => cb(e.payload));
}

export function onGameOutput(cb: (line: string) => void): Promise<UnlistenFn> {
  return listen<string>("game://output", (e) => cb(e.payload));
}

export function onGameExit(cb: (code: number) => void): Promise<UnlistenFn> {
  return listen<number>("game://exit", (e) => cb(e.payload));
}

export type { InstalledMod };
