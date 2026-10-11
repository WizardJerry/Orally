import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import AudioLines from "lucide-react/dist/esm/icons/audio-lines.mjs";
import SlidersHorizontal from "lucide-react/dist/esm/icons/sliders-horizontal.mjs";
import Settings2 from "lucide-react/dist/esm/icons/settings-2.mjs";
import Plus from "lucide-react/dist/esm/icons/plus.mjs";
import Upload from "lucide-react/dist/esm/icons/upload.mjs";
import Download from "lucide-react/dist/esm/icons/download.mjs";
import RotateCcw from "lucide-react/dist/esm/icons/rotate-ccw.mjs";
import Ellipsis from "lucide-react/dist/esm/icons/ellipsis.mjs";
import Play from "lucide-react/dist/esm/icons/play.mjs";
import Save from "lucide-react/dist/esm/icons/save.mjs";
import LoaderCircle from "lucide-react/dist/esm/icons/loader-circle.mjs";
import ChevronRight from "lucide-react/dist/esm/icons/chevron-right.mjs";
import CircleAlert from "lucide-react/dist/esm/icons/circle-alert.mjs";
import Check from "lucide-react/dist/esm/icons/check.mjs";
import X from "lucide-react/dist/esm/icons/x.mjs";
import { Button } from "@/components/ui/button";
import { Kbd, KbdGroup } from "@/components/ui/kbd";
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";
import { Select, SelectTrigger, SelectValue, SelectContent, SelectItem } from "@/components/ui/select";
import { DropdownMenu, DropdownMenuTrigger, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator } from "@/components/ui/dropdown-menu";
import { AlertDialog, AlertDialogContent, AlertDialogHeader, AlertDialogTitle, AlertDialogDescription, AlertDialogFooter, AlertDialogCancel, AlertDialogAction } from "@/components/ui/alert-dialog";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ProfileEditor, IconButton } from "./components/profile-editor.jsx";
import { SettingsPage } from "./components/settings-page.jsx";
import { ConfigTestDialog } from "./components/config-test-dialog.jsx";
import { WorkspaceController } from "./workspace-controller.js";
import { nativeInvoke, nativeListen, redactError } from "./lib/runtime.js";
import { formatHotkey } from "./hotkey-capture.js";

export default function App() {
  const [controller] = useState(() => new WorkspaceController({ invoke: nativeInvoke,
    storage: { getItem: (key) => localStorage.getItem(key), setItem: (key, value) => localStorage.setItem(key, value) },
  }));
  const { editor, loading, saving, settingsSaving, settingsError, importing, status, contextVersion } = useSyncExternalStore(controller.subscribe, controller.getState);
  const [page, setPage] = useState("home");
  const [test, setTest] = useState({ open: false, mode: "voice" });
  const [confirmReload, setConfirmReload] = useState(false);
  const capture = useRef(null);
  const [captureState, setCaptureState] = useState({ busy: false });
  const importInput = useRef(null);
  const actionBusy = loading || saving || settingsSaving || importing || captureState.busy || !editor;
  const editDisabled = loading || importing;
  const onCaptureChange = useCallback((session) => { capture.current = session; setCaptureState({ busy: session?.busy ?? false }); }, []);
  const changeSettings = useCallback((mutate) => {
    controller.editSettings(mutate);
  }, [controller]);

  const darkMode = editor?.config.appearance.dark_mode ?? true;
  useEffect(() => {
    document.documentElement.classList.toggle("dark", darkMode);
    document.documentElement.style.colorScheme = darkMode ? "dark" : "light";
  }, [darkMode]);

  useEffect(function initializeWorkspace() {
    let disposed = false;
    const listeners = [];
    const unlisten = () => { listeners.splice(0).forEach((stop) => stop()); };
    controller.disposed = false;
    async function initialize() {
      let eventError;
      if (nativeListen) {
        try {
          const stop = await nativeListen("active-profile-changed", (event) => controller.queueSavedProfileSync(event.payload?.profile_id));
          if (disposed) { stop(); return; }
          listeners.push(stop);
          for (const [name, callback] of [
            ["config-changed", () => controller.queueConfigSync()],
            ["config-sync-error", (event) => controller.status(`文件同步失败：${redactError(event.payload, controller.snapshot())}`, true)],
          ]) {
            const stopSync = await nativeListen(name, callback);
            if (disposed) { stopSync(); return; }
            listeners.push(stopSync);
          }
        } catch (error) { eventError = error; }
      }
      if (disposed) return;
      await controller.load();
      if (eventError && !disposed) controller.status(`托盘配置同步未启用：${redactError(eventError, controller.snapshot())}`, true);
    }
    void initialize();
    const beforeUnload = (event) => {
      void capture.current?.cancel();
      if (controller.getState().editor?.dirty) { event.preventDefault(); event.returnValue = ""; }
    };
    const pageHide = () => { void capture.current?.cancel(); controller.clearConnections(); unlisten(); };
    const storageChanged = (event) => { if (event.key === "orally-config-editor-v1") controller.queueConfigSync(); };
    window.addEventListener("storage", storageChanged);
    window.addEventListener("beforeunload", beforeUnload);
    window.addEventListener("pagehide", pageHide);
    return () => {
      disposed = true; unlisten?.(); controller.dispose();
      window.removeEventListener("beforeunload", beforeUnload);
      window.removeEventListener("pagehide", pageHide);
      window.removeEventListener("storage", storageChanged);
    };
  }, [controller]);

  useEffect(function resetTransientContext() {
    setTest((current) => current.open ? { ...current, open: false } : current);
    void capture.current?.cancel();
  }, [contextVersion]);

  const save = useCallback(async () => {
    if (capture.current?.busy) return;
    const saved = await controller.save();
    if (saved) capture.current?.clearError();
  }, [controller]);

  useEffect(function bindSaveShortcut() {
    const keyDown = (event) => {
      if (event.repeat || capture.current?.busy || test.open || confirmReload) return;
      if (event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey && (event.code === "KeyS" || event.key.toLowerCase() === "s")) {
        event.preventDefault(); if (page === "home") void save();
      }
    };
    document.addEventListener("keydown", keyDown);
    return () => document.removeEventListener("keydown", keyDown);
  }, [save, test.open, confirmReload, page]);

  function navigate(next) { void capture.current?.cancel(); setTest((current) => ({ ...current, open: false })); setPage(next); }
  function openTest(mode = "voice") { if (!actionBusy) setTest({ open: true, mode }); }
  function reload() { if (editor?.dirty) setConfirmReload(true); else void controller.load(); }
  function newProfile() {
    controller.newProfile(); navigate("home");
    requestAnimationFrame(() => { const input = document.getElementById("config-name"); input?.focus(); input?.select(); });
  }
  function exportProfile() {
    const name = editor.activeProfile.name.trim().replace(/[<>:"/\\|?*]/g, "-") || "orally-config";
    const url = URL.createObjectURL(new Blob([JSON.stringify(editor.exportProfile(), null, 2)], { type: "application/json" }));
    const link = document.createElement("a"); link.href = url; link.download = `${name}.json`; link.click();
    setTimeout(() => URL.revokeObjectURL(url), 0);
    controller.status("配置已导出，其中包含填写的 API Key，请妥善保管。");
  }

  return <TooltipProvider delayDuration={350}>
    <div className="desktop-shell">
      <aside className="app-sidebar">
        <div className="app-brand"><span className="brand-symbol"><AudioLines aria-hidden="true" /></span><span className="brand-wordmark">Orally<span>语音输入</span></span></div>
        <Separator />
        <nav className="app-nav" aria-label="主导航">
          <span className="nav-label">工作空间</span>
          <Button variant={page === "home" ? "secondary" : "ghost"} className="nav-button" disabled={loading} onClick={() => navigate("home")}
            aria-current={page === "home" ? "page" : undefined} aria-label="模型配置"><SlidersHorizontal /><span>模型配置</span></Button>
          <Button variant={page === "settings" ? "secondary" : "ghost"} className="nav-button" disabled={loading} onClick={() => navigate("settings")}
            aria-current={page === "settings" ? "page" : undefined} aria-label="应用设置"><Settings2 /><span>应用设置</span></Button>
        </nav>
        <div className="sidebar-bottom">
          <div className="shortcut-summary"><span>开始 / 停止听写</span><KbdGroup className="mt-2 flex flex-wrap">{formatHotkey(editor?.config.hotkey.preset ?? "ctrl-alt-space").split(" + ").map((key) => <Kbd key={key}>{key}</Kbd>)}</KbdGroup></div>
          <Separator /><span className="sidebar-version">Orally <span>v0.1.0</span></span>
        </div>
      </aside>

      <main className="app-workspace">
        <header className="workspace-header">
          <div className="flex items-center gap-2 text-sm text-muted-foreground"><AudioLines className="size-4" /><span>工作空间</span><ChevronRight className="size-3" /><h1 className="font-medium text-foreground">{page === "home" ? "模型配置" : "应用设置"}</h1></div>
          {page === "settings" && <span className="flex items-center gap-2 text-xs text-muted-foreground" role="status" aria-live="polite">
            {settingsSaving ? <LoaderCircle className="size-3.5 animate-spin" /> : settingsError ? <CircleAlert className="size-3.5" /> : <Check className="size-3.5" />}
            {settingsSaving ? "正在保存…" : settingsError ? "保存失败" : "已自动保存"}
          </span>}
        </header>
        <div className="workspace-toolbar" hidden={page !== "home"}>
          <Select value={editor?.config.active_profile_id ?? ""} onValueChange={(id) => controller.selectProfile(id)} disabled={actionBusy}>
            <SelectTrigger id="config-select" className="profile-select" aria-label="切换配置"><SelectValue placeholder="载入配置…" /></SelectTrigger>
            <SelectContent>{editor?.config.profiles.map((profile) => <SelectItem key={profile.id} value={profile.id}>{profile.name || "未命名配置"}{editor.profileDirty(profile.id) ? " · 未保存" : ""}</SelectItem>)}</SelectContent>
          </Select>
          <div className="toolbar-file-actions">
            <Button variant="ghost" size="sm" disabled={actionBusy} onClick={newProfile}><Plus />新建</Button>
            <Button variant="ghost" size="sm" disabled={actionBusy} onClick={() => importInput.current?.click()}><Upload />导入</Button>
            <DropdownMenu><DropdownMenuTrigger asChild><Button variant="ghost" size="icon" className="size-8" aria-label="更多配置操作" disabled={actionBusy}><Ellipsis /></Button></DropdownMenuTrigger>
              <DropdownMenuContent align="end"><DropdownMenuItem onSelect={exportProfile}><Download />导出当前配置</DropdownMenuItem><DropdownMenuSeparator /><DropdownMenuItem onSelect={reload}><RotateCcw />重新载入</DropdownMenuItem></DropdownMenuContent>
            </DropdownMenu>
          </div>
          <div className="toolbar-primary-actions">
            <Button variant="outline" size="sm" disabled={actionBusy} onClick={() => openTest()}><Play />测试</Button>
            <Button size="sm" disabled={actionBusy} onClick={save}>{saving ? <LoaderCircle className="animate-spin" /> : <Save />}<span>{saving ? "保存中…" : "保存配置"}</span></Button>
          </div>
          <input ref={importInput} hidden type="file" accept=".json,.toml,application/json" onChange={async (event) => {
            const file = event.target.files?.[0]; event.target.value = "";
            if (file) { await controller.importFile(file); navigate("home"); }
          }} />
        </div>
        <div className="workspace-scroll"><div className="workspace-content">
          {status ? <Alert className={`workspace-alert ${status.error ? "border-foreground/50" : ""}`} role={status.error ? "alert" : "status"}>
            {status.error ? <CircleAlert className="size-4" /> : <Check className="size-4" />}
            <AlertDescription className="pr-6 break-words">{status.message}</AlertDescription>
            <IconButton label="关闭提示" className="absolute right-2 top-2 size-7" onClick={() => controller.status("")}><X /></IconButton>
          </Alert> : null}
          {!editor ? <div aria-label="正在载入配置" className="space-y-5 py-4"><Skeleton className="h-12 w-56" /><Skeleton className="h-72 w-full" /><Skeleton className="h-96 w-full" /></div> : <>
            <div hidden={page !== "home"}><ProfileEditor key={contextVersion} controller={controller} editor={editor} disabled={editDisabled} saving={saving} onTest={openTest} /></div>
            <div hidden={page !== "settings"}><SettingsPage config={editor.config} disabled={editDisabled || saving} onChange={changeSettings} onStatus={controller.status} onCaptureChange={onCaptureChange} active={page === "settings"} /></div>
          </>}
          <footer className="workspace-footer"><AudioLines className="size-3.5" /><span>Orally</span><span className="ml-auto">{page === "settings" ? settingsSaving ? "正在保存…" : settingsError ? "保存失败，已恢复原设置" : "设置自动保存" : editor?.dirty ? "修改尚未保存" : "所有更改已保存"}</span></footer>
        </div></div>
      </main>
    </div>
    <ConfigTestDialog open={test.open} onOpenChange={(open) => setTest((current) => ({ ...current, open }))} initialMode={test.mode} getSnapshot={controller.snapshot} loading={loading} />
    <AlertDialog open={confirmReload} onOpenChange={setConfirmReload}>
      <AlertDialogContent><AlertDialogHeader><AlertDialogTitle>重新载入配置？</AlertDialogTitle><AlertDialogDescription>所有未保存的修改都会被已保存的配置替换。</AlertDialogDescription></AlertDialogHeader>
        <AlertDialogFooter><AlertDialogCancel>保留修改</AlertDialogCancel><AlertDialogAction onClick={() => void controller.load()}>重新载入</AlertDialogAction></AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </TooltipProvider>;
}
