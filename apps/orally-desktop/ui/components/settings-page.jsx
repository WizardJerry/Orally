import { useEffect, useRef, useState } from "react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Checkbox } from "@/components/ui/checkbox";
import { nativeInvoke, redactError } from "@/lib/runtime";
import { DEFAULT_CONFIG } from "../editor-state.js";
import { HotkeyCaptureSession, formatHotkey } from "../hotkey-capture.js";

function SwitchSetting({ id, title, description, checked, disabled, onCheckedChange }) {
  return (
    <div className="flex items-center justify-between gap-6 py-1">
      <div className="space-y-1">
        <Label htmlFor={id}>{title}</Label>
        {description ? <p className="text-xs leading-relaxed text-muted-foreground">{description}</p> : null}
      </div>
      <Switch id={id} checked={Boolean(checked)} disabled={disabled} onCheckedChange={onCheckedChange} />
    </div>
  );
}

const PHASE_LABELS = { preparing: "准备录入…", capturing: "按下组合键…", restoring: "完成录入…" };
const PHASE_HINTS = { preparing: "正在暂停全局快捷键。", capturing: "按下新的组合键，Esc 取消。", "awaiting-release": "松开按键完成录入，Esc 取消。", restoring: "正在恢复全局快捷键。" };

export function SettingsPage({ config, disabled = false, onChange, onStatus, onCaptureChange, active = true }) {
  const callbacks = useRef({ config, onChange, onStatus, onCaptureChange, active });
  callbacks.current = { config, onChange, onStatus, onCaptureChange, active };
  const mounted = useRef(false);
  const [, refreshCapture] = useState(0);
  const [capture] = useState(() => new HotkeyCaptureSession({
    setActive: (isActive) => nativeInvoke ? nativeInvoke("set_hotkey_capture", { active: isActive }) : Promise.resolve(),
    onCommit: (value) => {
      if (!mounted.current || !callbacks.current.active) return;
      callbacks.current.onChange((draft) => { draft.hotkey.preset = value; });
    },
    onChange: (session) => {
      if (!mounted.current) return;
      refreshCapture((revision) => revision + 1);
      callbacks.current.onCaptureChange(session);
      if (session.error) {
        const reason = redactError(session.error, callbacks.current.config);
        callbacks.current.onStatus(session.restorationFailed
          ? `原快捷键恢复失败：${reason}。请重新录入。`
          : `快捷键录入失败：${reason}`, true);
      }
    },
  }));

  useEffect(function bindHotkeyCapture() {
    mounted.current = true;
    callbacks.current.onCaptureChange(capture);
    function consumeKeyDown(event) {
      if (!capture.busy) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      void capture.handleKey(event);
    }
    function consumeKeyUp(event) {
      if (!capture.busy) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      void capture.handleKeyUp(event);
    }
    function cancelCapture() { void capture.cancel(); }
    function cancelWhenHidden() { if (document.hidden) cancelCapture(); }
    document.addEventListener("keydown", consumeKeyDown, true);
    document.addEventListener("keyup", consumeKeyUp, true);
    document.addEventListener("visibilitychange", cancelWhenHidden);
    window.addEventListener("blur", cancelCapture);
    window.addEventListener("pagehide", cancelCapture);
    window.addEventListener("beforeunload", cancelCapture);
    return function releaseHotkeyCapture() {
      mounted.current = false;
      document.removeEventListener("keydown", consumeKeyDown, true);
      document.removeEventListener("keyup", consumeKeyUp, true);
      document.removeEventListener("visibilitychange", cancelWhenHidden);
      window.removeEventListener("blur", cancelCapture);
      window.removeEventListener("pagehide", cancelCapture);
      window.removeEventListener("beforeunload", cancelCapture);
      void capture.cancel();
      callbacks.current.onCaptureChange(null);
    };
  }, [capture]);

  useEffect(function cancelUnavailableCapture() {
    if (!active || disabled) void capture.cancel();
  }, [active, disabled, capture]);

  const phase = capture.phase;
  const locked = disabled || capture.busy;
  const directOutput = config.output.direct_output ?? true;
  const combination = formatHotkey(capture.staged ?? config.hotkey.preset);
  const captureLabel = PHASE_LABELS[phase] ?? combination;
  const captureHint = PHASE_HINTS[phase] ?? "点击录入新的组合键。";
  const updateOutput = (key, value) => onChange((draft) => { draft.output[key] = value; });
  const updatePrivacy = (key, value) => onChange((draft) => { draft.privacy[key] = value; });

  return (
    <div className="settings-page space-y-6">
      <Card aria-labelledby="shortcuts-title">
        <CardHeader className="flex flex-row items-start justify-between gap-4 space-y-0">
          <div className="space-y-1.5">
            <CardTitle id="shortcuts-title">快捷键</CardTitle>
            <CardDescription>在任意应用中开始或停止语音输入。</CardDescription>
          </div>
          <Button id="restore-shortcuts" variant="ghost" size="sm" disabled={locked} onClick={() => {
            onChange((draft) => { draft.hotkey.preset = DEFAULT_CONFIG.hotkey.preset; });
            capture.clearError();
          }}>恢复默认</Button>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-4">
            <Label htmlFor="hotkey-capture">开始/停止听写</Label>
            <Button id="hotkey-capture" variant="outline" className="min-w-44 font-mono" disabled={disabled || phase === "restoring"}
              aria-label={`录入开始或停止听写的快捷键，当前组合：${combination}`}
              aria-describedby="hotkey-capture-state" aria-pressed={capture.busy} aria-busy={phase === "preparing" || phase === "restoring"}
              onClick={() => { if (capture.busy) void capture.cancel(); else void capture.start(); }}
              onBlur={() => { if (["preparing", "capturing", "awaiting-release"].includes(capture.phase)) void capture.cancel(); }}>
              {captureLabel}
            </Button>
          </div>
          <p id="hotkey-capture-state" className="text-xs text-muted-foreground" role="status" aria-live="polite">{captureHint}</p>
          {capture.restorationFailed ? <Alert variant="destructive"><AlertDescription>原快捷键恢复失败，请重新录入。</AlertDescription></Alert> : null}
        </CardContent>
      </Card>

      <Card aria-labelledby="output-title">
        <CardHeader>
          <CardTitle id="output-title">输出</CardTitle>
          <CardDescription>选择文字的交付方式与剪贴板行为。</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-4">
            <div className="space-y-1">
              <Label htmlFor="output-direct">直接输出</Label>
              <p id="output-direct-hint" className="text-xs leading-relaxed text-muted-foreground">开启后直接输入文字；关闭后使用剪贴板粘贴。</p>
            </div>
            <Select value={directOutput ? "direct" : "clipboard"} disabled={locked} onValueChange={(value) => updateOutput("direct_output", value === "direct")}>
              <SelectTrigger id="output-direct" className="min-w-44" aria-describedby="output-direct-hint"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="direct">开启 · 直接输出</SelectItem>
                <SelectItem value="clipboard">关闭 · 剪贴板粘贴</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <SwitchSetting id="output-auto-copy" title="输出自动复制" description={directOutput ? "输出时同时将结果复制到剪贴板。" : "剪贴板粘贴会自动保留结果；开启直接输出后可调整。"} checked={!directOutput || config.output.auto_copy} disabled={locked || !directOutput} onCheckedChange={(value) => updateOutput("auto_copy", value)} />
        </CardContent>
      </Card>

      <Card aria-labelledby="privacy-title">
        <CardHeader>
          <CardTitle id="privacy-title">隐私</CardTitle>
          <CardDescription>控制外部请求与本地历史。</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <SwitchSetting id="privacy-allow-external-requests" title="允许外部 AI 请求" description="关闭后不会向配置的外部服务发送内容。" checked={config.privacy.allow_external_requests} disabled={locked} onCheckedChange={(value) => updatePrivacy("allow_external_requests", value)} />
          <SwitchSetting id="privacy-history-enabled" title="保存本地历史" checked={config.privacy.history_enabled} disabled={locked} onCheckedChange={(value) => updatePrivacy("history_enabled", value)} />
          <Separator />
          <div className="space-y-2">
            <Label htmlFor="privacy-history-path">历史路径</Label>
            <Input id="privacy-history-path" placeholder="留空则使用默认 history.jsonl" spellCheck={false} disabled={locked} value={config.privacy.history_path ?? ""} onChange={(event) => updatePrivacy("history_path", event.target.value.trim() ? event.target.value : null)} />
          </div>
        </CardContent>
      </Card>
      <Card aria-labelledby="logging-title">
        <CardHeader>
          <CardTitle id="logging-title">诊断日志</CardTitle>
          <CardDescription>默认以 DEBUG 最高详细度记录网络请求与录音元信息，按本机日期写入应用文件夹中的 logs 目录。</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-xs leading-relaxed text-muted-foreground">包含 HTTP 状态码、协议、耗时、数据大小和录音时长；不包含音频、文字正文、提示词、请求头或 API Key。此设置独立于本地历史。</p>
          <SwitchSetting id="logging-auto-cleanup" title="自动清理日志" description="默认仅保留当天；跨天后清理过期文件。关闭后按天持续保留。" checked={config.logging.auto_cleanup} disabled={locked} onCheckedChange={(value) => onChange((draft) => { draft.logging.auto_cleanup = value; })} />
          <div className="flex flex-wrap items-center justify-between gap-4">
            <Label htmlFor="logging-retention-days">保留天数（含今天）</Label>
            <Select value={String(config.logging.retention_days)} disabled={locked || !config.logging.auto_cleanup} onValueChange={(value) => onChange((draft) => { draft.logging.retention_days = Number(value); })}>
              <SelectTrigger id="logging-retention-days" className="min-w-44"><SelectValue /></SelectTrigger>
              <SelectContent>
                {[...new Set([1, 3, 7, 30, config.logging.retention_days])].sort((a, b) => a - b).map((days) => (
                  <SelectItem key={days} value={String(days)}>{days === 1 ? "1 天 · 仅当天" : `${days} 天`}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </CardContent>
      </Card>
      <Card aria-labelledby="other-title">
        <CardHeader><CardTitle id="other-title">其他</CardTitle></CardHeader>
        <CardContent>
          <div className="flex items-center justify-between gap-6 py-1">
            <Label htmlFor="dark-mode">深色模式</Label>
            <Checkbox id="dark-mode" checked={config.appearance.dark_mode} disabled={locked}
              onCheckedChange={(checked) => onChange((draft) => { draft.appearance.dark_mode = checked === true; })} />
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
