import { ConfigEditor, DEFAULT_CONFIG } from "./editor-state.js";
import { testServiceConnection } from "./service-tests.js";
import { redactError } from "./lib/runtime.js";

export const PREVIEW_STORAGE_KEY = "orally-config-editor-v1";
const SETTINGS_SECTIONS = ["output", "audio", "hotkey", "privacy", "appearance", "logging"];
export const settingsOf = (config) => Object.fromEntries(SETTINGS_SECTIONS.map((key) => [key, structuredClone(config[key])]));

// The React view observes this controller. All edits still belong to ConfigEditor,
// including edits made while a save or an external tray selection is in flight.
export class WorkspaceController {
  constructor({ invoke, storage, testConnection = testServiceConnection } = {}) {
    this.invoke = invoke;
    this.storage = storage;
    this.testConnection = testConnection;
    this.listeners = new Set();
    this.connections = new Map();
    this.operation = 0;
    this.syncVersion = 0;
    this.pendingSync = false;
    this.syncing = false;
    this.disposed = false;
    this.pendingSettings = false;
    this.settingsTask = null;
    this.state = { editor: null, loading: false, saving: false, settingsSaving: false, settingsError: null, importing: false, status: null, contextVersion: 0 };
  }

  subscribe = (listener) => { this.listeners.add(listener); return () => this.listeners.delete(listener); };
  getState = () => this.state;
  snapshot = () => this.state.editor?.snapshot();
  emit(patch = {}) {
    if (this.disposed) return;
    this.state = { ...this.state, ...patch };
    this.listeners.forEach((listener) => listener());
  }
  status = (message, error = false) => this.emit({ status: message ? { message, error } : null });

  edit = (mutate, connectionId) => {
    if (!this.state.editor || this.state.loading || this.state.importing) return;
    mutate(this.state.editor);
    if (connectionId !== undefined) this.invalidateConnection(connectionId);
    this.emit({ status: null });
  };

  editSettings = (mutate) => {
    if (!this.state.editor || this.state.loading || this.state.importing) return;
    mutate(this.state.editor.config);
    this.clearConnections();
    this.pendingSettings = true;
    this.syncVersion += 1;
    this.emit({ settingsSaving: true, settingsError: null, status: null });
    // Coalesce same-turn changes, then serialize writes so late replies cannot
    // overwrite newer input. Model drafts never enter save_settings.
    if (!this.settingsTask) {
      this.settingsTask = Promise.resolve().then(() => this.flushSettings()).finally(() => {
        this.settingsTask = null;
        void this.syncSavedProfile();
      });
    }
  };

  async flushSettings() {
    while (this.pendingSettings && !this.disposed) {
      this.pendingSettings = false;
      const editor = this.state.editor;
      const submitted = settingsOf(editor.config);
      const base = settingsOf(editor.saved);
      try {
        let applied = submitted;
        if (this.invoke) applied = await this.invoke("save_settings", { settings: submitted, base }) ?? submitted;
        else {
          const stored = this.storage?.getItem(PREVIEW_STORAGE_KEY);
          const saved = stored ? JSON.parse(stored) : structuredClone(editor.saved);
          const current = settingsOf(new ConfigEditor(saved).config);
          for (const section of SETTINGS_SECTIONS) {
            for (const key of Object.keys(submitted[section])) {
              if (JSON.stringify(base[section][key]) !== JSON.stringify(submitted[section][key])) {
                current[section][key] = structuredClone(submitted[section][key]);
              }
            }
          }
          applied = current;
          Object.assign(saved, current);
          this.storage.setItem(PREVIEW_STORAGE_KEY, JSON.stringify(saved));
        }
        if (this.disposed) return;
        for (const section of SETTINGS_SECTIONS) {
          for (const key of Object.keys(submitted[section])) {
            if (JSON.stringify(editor.config[section][key]) === JSON.stringify(submitted[section][key])) {
              editor.config[section][key] = structuredClone(applied[section][key]);
            }
          }
        }
        Object.assign(editor.saved, structuredClone(applied));
        this.emit({ settingsError: null });
      } catch (error) {
        if (this.disposed) return;
        // Roll back only the failed values; preserve input typed during this write.
        for (const section of SETTINGS_SECTIONS) {
          for (const key of Object.keys(submitted[section])) {
            if (JSON.stringify(editor.config[section][key]) === JSON.stringify(submitted[section][key])) {
              editor.config[section][key] = structuredClone(editor.saved[section][key]);
            }
          }
        }
        const message = `设置保存失败：${redactError(error, this.snapshot())}`;
        this.emit({ settingsError: message, status: { message, error: true } });
      }
    }
    this.emit({ settingsSaving: false });
  }

  contextChanged(editor = this.state.editor) {
    this.clearConnections();
    this.emit({ editor, contextVersion: this.state.contextVersion + 1 });
  }
  selectProfile(id) { this.state.editor.selectProfile(id); this.contextChanged(); }
  newProfile() {
    const profile = this.state.editor.newProfile();
    this.contextChanged();
    this.status("已新建配置");
    return profile;
  }

  async load() {
    if (this.state.loading || this.state.saving || this.state.importing) return;
    if (this.settingsTask) await this.settingsTask;
    const operation = ++this.operation;
    this.emit({ loading: true });
    try {
      const saved = this.invoke ? await this.invoke("get_config") : this.storage?.getItem(PREVIEW_STORAGE_KEY);
      const config = this.invoke ? saved : saved ? JSON.parse(saved) : DEFAULT_CONFIG;
      if (this.disposed || operation !== this.operation) return;
      this.contextChanged(new ConfigEditor(config));
      this.emit({ settingsError: null });
      this.status("");
    } catch (error) {
      if (this.disposed || operation !== this.operation) return;
      if (!this.state.editor) {
        const editor = new ConfigEditor();
        editor.markUnsaved();
        this.contextChanged(editor);
      }
      this.status(`载入失败：${redactError(error, this.snapshot())}。当前草稿已保留，可重新载入或导入配置。`, true);
    } finally {
      if (!this.disposed && operation === this.operation) {
        this.emit({ loading: false });
        void this.syncSavedProfile();
      }
    }
  }

  async save() {
    if (this.settingsTask) await this.settingsTask;
    const { editor, loading, saving, importing } = this.state;
    if (!editor || loading || saving || importing) return false;
    if (!editor.activeProfile.name.trim()) { this.status("请填写当前配置名称", true); return false; }
    const snapshot = editor.snapshot();
    const operation = ++this.operation;
    this.emit({ saving: true });
    try {
      if (this.invoke) await this.invoke("save_config", { config: snapshot });
      else this.storage.setItem(PREVIEW_STORAGE_KEY, JSON.stringify(snapshot));
      if (this.disposed || operation !== this.operation) return false;
      const acknowledged = structuredClone(snapshot);
      // Model saves acknowledge models only. Auto-saved settings may already
      // have advanced while this request was pending.
      Object.assign(acknowledged, settingsOf(editor.saved));
      editor.markSaved(acknowledged);
      this.status("配置已保存");
      return true;
    } catch (error) {
      this.status(`保存失败：${redactError(error, snapshot)}`, true);
      return false;
    } finally {
      if (!this.disposed && operation === this.operation) {
        this.emit({ saving: false });
        void this.syncSavedProfile();
      }
    }
  }

  async importFile(file) {
    if (this.settingsTask) await this.settingsTask;
    if (!this.state.editor || this.state.loading || this.state.saving || this.state.importing) return;
    const operation = ++this.operation;
    this.emit({ importing: true });
    try {
      const content = await file.text();
      let value;
      if (file.name.toLowerCase().endsWith(".toml")) {
        if (!this.invoke) throw new Error("TOML 导入需要桌面版，当前可导入 JSON 配置");
        value = await this.invoke("parse_config_document", { content });
      } else value = JSON.parse(content.replace(/^\uFEFF/, ""));
      if (this.disposed || operation !== this.operation) return;
      const count = this.state.editor.import(value);
      this.contextChanged();
      this.status(`已导入 ${count} 个配置`);
    } catch (error) {
      // Import parse failures can echo unknown credentials from the file.
      this.status("导入失败，请检查文件是否为有效的 Orally JSON 或 TOML 配置。", true);
    } finally {
      if (!this.disposed && operation === this.operation) {
        this.emit({ importing: false });
        void this.syncSavedProfile();
      }
    }
  }

  queueSavedProfileSync = (id) => {
    if (typeof id !== "string" || !id) return;
    this.queueConfigSync();
  };

  queueConfigSync = () => {
    this.syncVersion += 1;
    this.pendingSync = true;
    void this.syncSavedProfile();
  };

  async syncSavedProfile() {
    if (this.disposed || this.syncing || !this.pendingSync || this.state.loading || this.state.saving || this.state.importing || this.state.settingsSaving) return;
    this.syncing = true;
    try {
      while (this.pendingSync && !this.state.loading && !this.state.saving && !this.state.importing && !this.state.settingsSaving && !this.disposed) {
        this.pendingSync = false;
        const version = this.syncVersion;
        const operation = this.operation;
        const stored = this.invoke ? await this.invoke("get_config") : this.storage?.getItem(PREVIEW_STORAGE_KEY);
        const config = this.invoke ? stored : stored ? JSON.parse(stored) : DEFAULT_CONFIG;
        if (this.disposed) return;
        if (version !== this.syncVersion || operation !== this.operation) { this.pendingSync = true; continue; }
        const editor = this.state.editor ?? new ConfigEditor(config);
        const before = JSON.stringify([editor.config.profiles, editor.config.active_profile_id]);
        editor.acceptExternalSavedConfig(config);
        if (before !== JSON.stringify([editor.config.profiles, editor.config.active_profile_id])) {
          this.contextChanged(editor);
          this.status(`配置已同步${editor.dirty ? "，未保存的修改已保留" : ""}`);
        } else {
          const recovered = /^(文件同步失败|同步配置失败)：/.test(this.state.status?.message ?? "");
          this.emit({ editor, settingsError: null, ...(recovered ? { status: null } : {}) });
        }
      }
    } catch (error) {
      this.status(`同步配置失败：${redactError(error, this.snapshot())}`, true);
    } finally {
      this.syncing = false;
      if (this.pendingSync) void this.syncSavedProfile();
    }
  }

  connectionKey(id = null) { return `${this.state.editor?.config.active_profile_id}:${id ?? "asr"}`; }
  connection(id = null) { return this.connections.get(this.connectionKey(id)); }
  invalidateConnection(id = null) {
    const key = this.connectionKey(id);
    this.connections.get(key)?.controller.abort();
    this.connections.delete(key);
  }
  clearConnections() {
    for (const test of this.connections.values()) test.controller.abort();
    this.connections.clear();
  }
  async runConnection(id = null) {
    if (!this.state.editor || this.state.loading || this.connection(id)?.busy) return;
    const snapshot = this.snapshot();
    const key = this.connectionKey(id);
    const test = { busy: true, controller: new AbortController(), message: "正在测试连接…" };
    this.connections.set(key, test);
    this.emit();
    const current = () => !this.disposed && this.connectionKey(id) === key && this.connections.get(key) === test;
    try {
      if (!snapshot.privacy.allow_external_requests) throw new Error("请先在应用设置中允许外部 AI 请求");
      const result = this.invoke
        ? await this.invoke("test_service_connection", { config: snapshot, modelId: id })
        : await this.testConnection(snapshot, id, { signal: test.controller.signal });
      if (!current()) return;
      Object.assign(test, { success: true, message: `${redactError(result.message, snapshot)} · ${Math.round(result.elapsed_ms)} ms` });
    } catch (error) {
      if (!current()) return;
      Object.assign(test, { error: true, message: `连接失败：${redactError(error, snapshot)}` });
    } finally {
      if (current()) { test.busy = false; this.emit(); }
    }
  }

  dispose() {
    this.disposed = true;
    this.operation += 1;
    this.clearConnections();
  }
}
