// 与 Rust 后端对应的类型定义

export type AuthMode = "offline" | "microsoft";

export interface Settings {
  game_dir: string;
  java_path: string | null;
  memory_mb: number;
  download_concurrency: number;
  auth_mode: AuthMode;
  offline_username: string;
  microsoft_account: MicrosoftAccount | null;
  microsoft_client_id: string;
  auto_install_mod_deps: boolean;
}

export interface MicrosoftAccount {
  access_token: string;
  refresh_token: string;
  uuid: string;
  username: string;
}

export interface RemoteVersion {
  id: string;
  version_type: string;
  release_time: string;
}

export interface InstalledVersion {
  id: string;
  mc_version: string;
  loader: string | null;
  loader_version: string | null;
  installed: boolean;
}

export interface InstallSummary {
  version_id: string;
  files_downloaded: number;
  files_skipped: number;
  assets_count: number;
  libraries_count: number;
  natives_extracted: boolean;
}

export interface DeviceCode {
  device_code: string;
  user_code: string;
  verification_uri: string;
  expires_in: number;
  interval: number;
  message: string;
}

export interface ModHit {
  project_id: string;
  title: string;
  description: string;
  downloads: number;
  icon_url: string | null;
  versions: string[];
  categories: string[];
  project_type: string;
}

export interface ModFile {
  url: string;
  filename: string;
  size: number;
  hashes: Record<string, string>;
  primary?: boolean;
}

export interface ModDependency {
  project_id: string | null;
  version_id: string | null;
  dependency_type: string;
}

export interface ModVersion {
  id: string;
  name: string;
  version_number: string;
  game_versions: string[];
  loaders: string[];
  files: ModFile[];
  dependencies: ModDependency[];
}

export interface InstalledMod {
  project_id: string;
  title: string;
  filename: string;
  size: number;
  is_dependency: boolean;
}

export interface ModInstallReportView {
  installed: InstalledMod[];
  unresolved: string[];
}

export interface DownloadProgress {
  label: string;
  done: number;
  total: number;
}
