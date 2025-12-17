---
"lovshot": minor
---

feat(permission): 启动时检查 macOS 屏幕录制权限

- 新增 permission 模块检测/请求屏幕录制权限
- 启动时自动检查权限状态，无权限则显示权限请求窗口
- 权限窗口包含授权步骤和故障排除指南（包括删除旧授权后重新授权的说明）
- 添加 NSScreenCaptureUsageDescription 到 Info.plist
