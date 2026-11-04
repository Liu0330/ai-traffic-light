# AI Traffic Light

<div align="center">

一个轻量、原生、始终置顶的 Windows AI 工作状态悬浮灯。

同时观察 Cursor、Codex、WorkBuddy、Trae、Windsurf、ChatGPT 与 VS Code，清楚显示 AI 正在工作、等待确认、发生异常或处于空闲状态。

[English](README_EN.md) · [快速开始](#快速开始) · [状态接口](#http-状态接口) · [手机通知](#手机通知) · [参与贡献](CONTRIBUTING.md)

![Windows](https://img.shields.io/badge/Windows-10%2F11-0078D4?logo=windows)
![Rust](https://img.shields.io/badge/Rust-native-000000?logo=rust)
![License](https://img.shields.io/badge/license-MIT-22c55e)
![Version](https://img.shields.io/badge/version-0.4.0-6366f1)

</div>

![AI Traffic Light 展开预览](docs/images/preview-expanded.png)

## 为什么做它

同时让多个 AI 编程工具执行任务时，最费注意力的往往不是任务本身，而是不停切换窗口确认“谁还在工作、谁正等我、谁出了问题”。AI Traffic Light 把这些状态压缩成桌面边缘的一组交通灯：一眼看懂，不打断当前工作。

## 功能亮点

- **小而安静**：紧凑模式逻辑尺寸仅 `94 × 50`，不占任务栏，不抢输入焦点。
- **多 AI 聚合**：红、黄、绿可同时点亮；数量角标和悬停面板展示每个应用的状态。
- **自动识别**：检测 Cursor、Codex、WorkBuddy、Trae、Windsurf、ChatGPT 和 VS Code 进程。
- **开放接口**：任何脚本、IDE 插件或 Agent 都能通过本地 HTTP 接口精确上报状态。
- **手机提醒**：支持 ntfy、Bark 和通用 Webhook；内置去重与冷却，默认只推送等待和异常。
- **原生高清**：Win32 + Rust，Per-monitor DPI 感知，ClearType 文本和高分屏缩放。
- **低资源占用**：单文件程序，无 WebView、Electron 或常驻运行时。
- **桌面友好**：始终置顶、位置记忆、多显示器安全定位、单实例运行。

## 状态含义

| 颜色 | 状态 | 含义 |
| --- | --- | --- |
| 🟢 绿色 | `working` | AI 正在生成、执行或处理任务 |
| 🟡 黄色 | `waiting` | 需要确认、授权、输入或人工审阅 |
| 🔴 红色 | `error` | 执行失败、网络异常或需要处理 |
| ⚫ 熄灭 | `idle` | 当前没有活动任务 |

多个 AI 处于不同状态时，对应的灯会同时点亮。将鼠标移到悬浮窗上，会平滑展开每个应用的详细状态。

## 快速开始

### 下载运行

1. 从 GitHub Releases 下载最新的 `ai-traffic-light.exe`。
2. 双击运行。程序会出现在当前显示器顶部，不会显示任务栏按钮。
3. 启动 Cursor、Codex 等工具，悬浮灯会自动识别活动进程。

Windows SmartScreen 可能会提示未知发布者，因为当前发行文件尚未购买代码签名证书。你可以自行从源码构建并核对产物。

### 从源码构建

需要 Windows 10/11 与稳定版 [Rust 工具链](https://www.rust-lang.org/tools/install)。

```powershell
git clone https://github.com/Liu0330/ai-traffic-light.git
cd ai-traffic-light
.\build.ps1
```

构建结果位于：

```text
target\release\ai-traffic-light.exe
```

也可以直接运行：

```powershell
cargo run --release
```

## 使用方式

| 操作 | 效果 |
| --- | --- |
| 鼠标悬停 | 展开多 AI 状态列表 |
| 拖动悬浮窗 | 移动位置，自动保存 |
| 单击交通灯 | 手动切换对应状态 |
| 双击 | 返回自动识别模式 |
| 右键 | 打开状态、通知测试与退出菜单 |

## HTTP 状态接口

服务只监听本机回环地址：`http://127.0.0.1:17321`。

### 上报一个 AI 的状态

```powershell
$body = @{
  app    = 'Cursor'
  status = 'working'
  label  = '正在修改代码'
  detail = '实现登录页面'
  ttl    = 120
} | ConvertTo-Json

Invoke-RestMethod `
  -Uri 'http://127.0.0.1:17321/status' `
  -Method Post `
  -ContentType 'application/json' `
  -Body $body
```

请求字段：

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `app` | 推荐 | AI/编辑器名称；同名应用会更新原有状态 |
| `status` | 是 | `working`、`waiting`、`error` 或 `idle` |
| `label` | 否 | 面板中的主文案 |
| `detail` | 否 | 更具体的任务信息 |
| `ttl` | 否 | 状态有效秒数，范围 `1–86400`，默认 `120` |

### 查询全部状态

```powershell
Invoke-RestMethod 'http://127.0.0.1:17321/status'
```

返回示例：

```json
{
  "status": "working",
  "label": "3 个 AI 软件",
  "detail": "1 异常 · 1 等待 · 1 工作",
  "source": "api",
  "apps": [
    {"app":"Cursor","status":"working","label":"正在工作","detail":"重构 UI","source":"api"},
    {"app":"Codex","status":"waiting","label":"等待确认","detail":"需要运行测试","source":"api"},
    {"app":"WorkBuddy","status":"error","label":"执行异常","detail":"网络连接失败","source":"api"}
  ]
}
```

接口支持浏览器跨域请求，可直接接入本地网页、脚本、IDE 插件和自动化工具。状态 TTL 到期后会自动清理，避免留下“假工作”状态。

## 手机通知

运行：

```powershell
.\mobile-notify\setup-mobile.ps1
```

也可以双击 `mobile-notify\setup-mobile.cmd`，按提示选择：

- **ntfy**：iPhone 与 Android，设置简单，推荐使用随机私人主题。
- **Bark**：适合 iPhone，粘贴 Bark 提供的推送地址即可。
- **通用 Webhook**：接入自建服务、自动化平台或其他消息网关。

默认仅推送 `waiting,error`，相同状态不会重复推送，并带有冷却时间。配置保存在 `%LOCALAPPDATA%\AITrafficLight\notifications.json`，该文件可能包含推送地址或密钥，请勿提交或分享。

详细步骤见 [手机通知说明](mobile-notify/README.md)。

## 工作原理

```text
AI 软件进程 ──自动检测──┐
                       ├── 状态聚合 ── Win32 悬浮窗
脚本 / Hook ──HTTP API─┘       └── ntfy / Bark / Webhook
```

应用使用 Rust 编写，界面由 Win32 GDI 原生绘制。后台线程负责本机 HTTP 服务、进程检测与通知发送；UI 线程只处理窗口交互和绘制，因此不依赖浏览器内核。

## 隐私与安全

- 状态接口仅绑定 `127.0.0.1`，不会直接暴露到局域网或互联网。
- 自动模式只读取进程名称与活动情况，不读取编辑器文件、对话内容或代码。
- 手机通知仅在你主动配置后启用；通知内容可能发送到你选择的服务商。
- 请勿在 `label` 或 `detail` 中放入密钥、密码或敏感代码。

安全问题请按 [安全策略](SECURITY.md) 私下报告，不要直接创建公开 Issue。

## 项目结构

```text
.
├─ src/main.rs              # Win32 窗口、状态聚合、HTTP 服务与通知
├─ assets/app.ico           # 应用图标
├─ mobile-notify/           # 手机通知配置工具
├─ .github/workflows/       # Windows 自动构建与 Release
├─ build.rs                 # Windows 资源构建
├─ build.ps1                # 本地 Release 构建脚本
└─ Cargo.toml
```

## 路线图

- [ ] 更精确的 Cursor / Codex / WorkBuddy Hook 适配器
- [ ] 托盘图标与开机启动设置
- [ ] 可选主题、尺寸和停靠方向
- [ ] 已签名的 Windows 安装包
- [ ] 通知模板和免打扰时间段

欢迎通过 Issue 提交使用场景，也欢迎直接实现并发起 Pull Request。

## 参与贡献

请先阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。提交代码前至少运行：

```powershell
cargo fmt --check
cargo check
cargo build --release
```

## 许可证

本项目使用 [MIT License](LICENSE)。


