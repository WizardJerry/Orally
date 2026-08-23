---
status: accepted
---

# Keep all portable data with the application

In Portable Installation mode, every persistent Orally artifact lives within the application directory so copying that directory to another Windows computer carries Config, Service Connections, Workflows, Modules, Local History, and retained audio together. Only temporary processing files may use operating-system locations. This extends the earlier config-only portable behavior into a complete portability contract.

## Consequences

Portable mode must not silently place persistent data in `%APPDATA%`, browser-local storage, the registry, or another machine-specific location. Future encryption must remain application-managed and cross-platform so protected portable data can move with the directory.
