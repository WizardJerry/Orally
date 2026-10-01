---
status: accepted
---

[English](0004-keep-portable-data-with-the-application.md) |
[简体中文](0004-keep-portable-data-with-the-application.zh-CN.md)

# 将所有便携数据与应用程序放在一起

在 Portable Installation 模式下，Orally 的每个持久化产物都位于应用程序目录中，因此将该目录复制到另一台 Windows 计算机时，会一起携带 Config、Service Connections、Workflows、Modules、Local History 和保留的音频。只有临时处理文件可以使用操作系统位置。此方案把早期仅配置可便携的行为扩展为完整的便携性契约。

## 后果

Portable 模式不得静默地将持久化数据放入 `%APPDATA%`、浏览器本地存储、注册表或其他机器特定位置。未来的加密必须继续由应用程序管理并支持跨平台，使受保护的便携数据能够随目录移动。
