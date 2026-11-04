# Contributing

感谢你帮助改进 AI Traffic Light。小而明确的 Issue 和 Pull Request 最容易被快速审阅。

## 开始之前

- Bug 请说明 Windows 版本、显示缩放比例、显示器数量、复现步骤与预期行为。
- 新功能请先描述使用场景，尤其说明它是否会增加常驻资源占用或界面复杂度。
- 安全漏洞请遵循 [SECURITY.md](SECURITY.md)，不要创建公开 Issue。

## 本地开发

需要 Windows 10/11、Git 和稳定版 Rust 工具链。

```powershell
git clone https://github.com/Liu0330/ai-traffic-light.git
cd ai-traffic-light
cargo run --release
```

程序为单实例；调试新构建前请先从右键菜单退出正在运行的版本。

## 提交前检查

```powershell
cargo fmt --check
cargo check
cargo build --release
```

请同时手动确认：

1. 紧凑和悬停展开动画正常。
2. 100%、125%、150% 或更高缩放下文字清晰且不裁切。
3. 多显示器移动后窗口没有消失。
4. `GET /status` 与 `POST /status` 仍然兼容。
5. 未提交 `%LOCALAPPDATA%\AITrafficLight` 下的个人配置。

## 代码风格

- 使用 `cargo fmt` 统一 Rust 格式。
- 保持依赖精简；新增依赖时请解释为什么不能使用标准库或现有 Win32 API。
- UI 改动应继续遵守“小、安静、清楚”的产品原则。
- 用户可见文案优先使用清晰、短句式表达。

## Pull Request

PR 描述应包含：变更目的、实现方式、测试结果，以及界面变更的截图。一个 PR 尽量只解决一个问题。

