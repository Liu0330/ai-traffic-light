# Security Policy

## Supported version

安全修复优先应用于最新发布版本和 `main` 分支。

## Reporting a vulnerability

请不要通过公开 Issue 报告安全漏洞。请使用 GitHub 仓库的 **Security → Report a vulnerability** 私下提交，包含：

- 受影响的版本或提交；
- 复现步骤与最小示例；
- 可能的影响；
- 你建议的修复方式（如有）。

维护者会尽快确认报告、评估影响并协调修复与披露时间。

## Scope notes

- 本地 HTTP 服务设计为只监听 `127.0.0.1`。
- 手机通知配置可能包含秘密 URL 或令牌，不应提交到仓库。
- 请避免在状态文案和通知内容中发送密码、API 密钥或敏感代码。

