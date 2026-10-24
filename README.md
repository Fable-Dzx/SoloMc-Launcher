# SoloMC

**去中心化 · 极简 · 跨平台** 的 Minecraft 启动器（Tauri 2 + Rust + React）。

![platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-blue)
![build](https://img.shields.io/badge/build-GitHub%20Actions-brightgreen)
![license](https://img.shields.io/badge/license-MIT-green)

## 特性

- **从 Mojang 官方获取游戏**：直连 `launchermeta.mojang.com` 版本清单，安装完整版本（client / libraries / assets / natives）。
- **可调多线程下载**：并发数 1–32 可调，支持断点续传、SHA-1 校验、已存在文件自动跳过。
- **加载器一键安装**：Fabric / Forge / NeoForge，全部来自官方 Meta / Maven 接口。
- **Modrinth 模组集成**：官方 API 搜索、按 MC 版本筛选、下载，**自动补全 required 依赖**（BFS 递归、防环、去重）。
- **版本隔离**：每个实例（版本 + 加载器组合）拥有独立游戏目录，saves / mods / config 互不干扰。
- **去中心化**：
  - 离线账号默认（自定义用户名，无需注册、无需联网授权）；
  - 可选微软正版登录，走标准设备码流程（XBL → XSTS → Minecraft），`client_id` 由用户自行注册，无任何第三方中转；
  - 所有数据源均为官方公开 API：Mojang / Fabric Meta / Forge Maven / NeoForge Maven / Modrinth。
- **三平台源码 + GitHub Action 自动构建**：Windows（NSIS/MSI）、macOS（dmg/app，x86_64 + aarch64）、Linux（deb/AppImage）。

## 技术栈

| 层 | 技术 |
|---|---|
| 桌面壳 | [Tauri 2](https://tauri.app)（Rust） |
| 后端 | Rust：reqwest(rustls) / tokio / zip / sha1 / serde |
| 前端 | React 18 + TypeScript + Vite 6（无重型 UI 库，手写简约深色样式） |
| CI | GitHub Actions（tauri-action 官方 action） |

## 快速开始（开发）

要求：Node ≥ 20、Rust stable、Tauri 系统依赖。

```bash
# 1. 安装前端依赖
npm install

# 2. 生成并转换图标（一次即可）
python3 scripts/gen_icon.py
npm run icon

# 3. 开发模式（热更新）
npm run tauri dev

# 4. 生产构建
npm run tauri build
```

### Tauri 系统依赖

- **Ubuntu/Debian**：
  ```bash
  sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev \
    libjavascriptcoregtk-4.1-dev librsvg2-dev patchelf
  ```
- **macOS / Windows**：无需额外依赖（Windows 10/11 自带 WebView2）。

## GitHub Action 构建

推送 `v*` tag 或手动触发 `build` workflow，自动在三个平台构建并上传 Release 产物：

```bash
git tag v0.1.0 && git push origin v0.1.0
```

产物：`.msi` / `.exe`（Windows）、`.dmg` / `.app`（macOS，Intel + Apple Silicon）、`.deb` / `.AppImage`（Linux）。
详见 [.github/workflows/build.yml](.github/workflows/build.yml)。

## 使用说明

### 安装与启动

1. **游戏** 页：从 Mojang 官方清单选择版本（可搜索），选择加载器（原版 / Fabric / Forge / NeoForge），点「安装」。
2. 安装完成后实例出现在左侧「已安装实例」，点「▶ 启动」。
3. 首次启动需要 Java 17 或更高版本（设置页可指定路径，留空自动探测 PATH/JAVA_HOME）。

### 模组

1. **模组** 页：选择目标实例，输入关键词搜索 Modrinth。
2. 展开模组选择与 MC 版本匹配的版本，点「安装」。
3. 默认自动补全 required 依赖（可关），optional 依赖不自动安装。

### 账号

- 默认离线：设置页填用户名即可，UUID 按 Java 官方离线规则生成。
- 微软正版：设置页「微软登录」→ 浏览器打开链接输入设备码 → 完成授权。
  - 需要在 [Azure 门户](https://portal.azure.com/) 注册应用：新建应用 → 认证 → 平台「移动和桌面应用」→ 勾选 **Allow public client flows** → 把 **应用程序(客户端) ID** 填到设置页 `client_id`。这是官方标准流程，任何第三方都不经过你的凭据。

## 目录结构

```
solo-mc-launcher/
├── .github/workflows/build.yml   # 三平台 CI/CD
├── scripts/gen_icon.py           # 图标生成（纯标准库）
├── src/                          # 前端（React + TS）
│   ├── api.ts                    # Tauri invoke 封装
│   ├── types.ts                  # 与 Rust 共享的类型
│   └── components/               # 游戏 / 模组 / 设置 视图
└── src-tauri/                    # Rust 后端
    ├── src/
    │   ├── commands.rs           # Tauri 命令层
    │   └── launcher/
    │       ├── meta.rs           # Mojang 版本清单 / version.json / assets 解析
    │       ├── download.rs       # 多线程下载器（断点续传 + sha1）
    │       ├── install.rs        # 版本安装 / 继承合并 / natives 解压
    │       ├── fabric.rs         # Fabric Meta API
    │       ├── forge.rs          # Forge promotions + Maven
    │       ├── neoforge.rs       # NeoForge API + Maven
    │       ├── loaders.rs        # 加载器统一入口 / 实例扫描
    │       ├── modrinth.rs       # 搜索 / 安装 / 依赖补全
    │       ├── auth.rs           # 离线 UUID / 微软设备码
    │       ├── process.rs        # JVM 参数 / 子进程 / 日志
    │       └── settings.rs       # 本地配置持久化
    └── Cargo.toml
```

## 数据目录

| 路径 | 内容 |
|---|---|
| `~/Library/Application Support/SoloMC/` (mac) | 版本、assets、libraries、natives |
| `~/.local/share/SoloMC/` (linux) / `%APPDATA%\SoloMC` (win) | 同上 |
| `~/.config/solo-mc/settings.json` | 设置 |

删除数据目录即完全卸载，无任何残留服务。

## 安全

- 令牌仅保存在本机配置文件，游戏日志自动打码 access_token；
- 下载目标路径全部由本地生成，`zip` 解压有路径穿越防护；
- 不收集任何遥测数据。

## License

MIT
