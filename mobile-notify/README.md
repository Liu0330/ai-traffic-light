# 手机通知设置

双击 `setup-mobile.cmd`，按提示选择通知方式。

## 推荐：ntfy（iPhone / Android）

1. 在手机安装 ntfy；
2. 运行设置工具，选择 `1`；
3. 输入一个不容易猜到的私人主题，或直接回车自动生成；
4. 在手机 ntfy 中订阅设置工具显示的同一个主题。

ntfy 的主题相当于密码，不要使用姓名、手机号等容易猜到的内容。

## Bark（iPhone）

1. 在 iPhone 安装 Bark；
2. 从 Bark 复制形如 `https://api.day.app/你的密钥` 的推送地址；
3. 运行设置工具，选择 `2` 并粘贴地址。

## 通用 Webhook

选择 `3` 后输入 HTTPS Webhook 地址。程序会发送：

```json
{
  "status": "waiting",
  "title": "AI 等待你的确认",
  "message": "Cursor needs review · 由接口上报",
  "source": "api"
}
```

默认只推送 `waiting,error`。可选事件还有 `working,idle`，用英文逗号分隔。程序会对相同状态去重，并执行冷却限制。

配置保存在 `%LOCALAPPDATA%\AITrafficLight\notifications.json`。地址中可能包含推送密钥，请勿分享该文件。
