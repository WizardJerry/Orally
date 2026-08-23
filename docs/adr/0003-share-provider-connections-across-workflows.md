---
status: accepted
---

# Share provider connections across Workflows

Provider endpoints, protocols, and credentials are stored as reusable Service Connections rather than duplicated inside every Workflow. A Workflow references an ASR connection and an LLM connection while retaining its own model selections, ASR instructions, and Modules. This centralizes secret and endpoint changes without preventing Workflows from using different models or providers.

## Consequences

Editing a Service Connection affects every referencing Workflow and must show the affected Workflows before saving. Users can copy a connection when isolation is required. Referenced connections cannot be deleted until their references are replaced or removed.

Read-only Built-in Workflows bind through global default ASR and default LLM service slots so software updates can replace their structure without overwriting user-specific providers or credentials. Custom Workflows may inherit those slots or bind explicitly.
