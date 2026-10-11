import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import CircleAlert from "lucide-react/dist/esm/icons/circle-alert.mjs";
import CircleCheck from "lucide-react/dist/esm/icons/circle-check.mjs";
import LoaderCircle from "lucide-react/dist/esm/icons/loader-circle.mjs";
import Mic from "lucide-react/dist/esm/icons/mic.mjs";
import Play from "lucide-react/dist/esm/icons/play.mjs";
import Square from "lucide-react/dist/esm/icons/square.mjs";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { ConfigTestController } from "@/lib/config-test-controller.js";
import { nativeInvoke, nativeListen, redactError } from "@/lib/runtime.js";

const TITLES = { voice: "语音输入测试", asr: "语音识别测试", text: "文本后处理测试" };
const PHASES = { idle: "准备录音", starting: "开启麦克风", recording: "正在录音", processing: "录音已停止", complete: "测试完成", error: "测试未完成" };

function clockText(milliseconds) {
  const seconds = Math.floor(Math.max(0, milliseconds) / 1000);
  return `${String(Math.floor(seconds / 60)).padStart(2, "0")}:${String(seconds % 60).padStart(2, "0")}`;
}

export function ConfigTestDialog({ open, onOpenChange, initialMode = "voice", getSnapshot, loading = false }) {
  const [controller] = useState(() => new ConfigTestController({ invoke: nativeInvoke, listen: nativeListen }));
  const state = useSyncExternalStore(controller.subscribe, controller.getState, controller.getState);
  const [mode, setMode] = useState(() => TITLES[initialMode] ? initialMode : "voice");
  const [inputText, setInputText] = useState("");
  const [elapsed, setElapsed] = useState(0);
  const inputRef = useRef(null);
  const callbacks = useRef({ onOpenChange, getSnapshot });
  callbacks.current = { onOpenChange, getSnapshot };

  useEffect(function synchronizeDialog() {
    void controller.reset();
    if (open) setMode(TITLES[initialMode] ? initialMode : "voice");
    return function releaseDialogRecording() { void controller.reset(); };
  }, [controller, open, initialMode]);

  useEffect(function releaseRecordingOnPageExit() {
    function cancel() { void controller.reset(); }
    function visibilityChanged() { if (document.hidden) cancel(); }
    window.addEventListener("pagehide", cancel);
    document.addEventListener("visibilitychange", visibilityChanged);
    return function removePageListeners() {
      window.removeEventListener("pagehide", cancel);
      document.removeEventListener("visibilitychange", visibilityChanged);
      void controller.reset();
    };
  }, [controller]);

  useEffect(function closeCancelledNativeSession() {
    if (open && state.closeRequested) callbacks.current.onOpenChange(false);
  }, [open, state.closeRequested]);

  useEffect(function updateRecordingClock() {
    if (!open || state.phase !== "recording") return undefined;
    function tick() { setElapsed(controller.tick()); }
    tick();
    const interval = setInterval(tick, 200);
    return function clearRecordingClock() { clearInterval(interval); };
  }, [controller, open, state.phase]);

  function changeOpen(nextOpen) {
    if (!nextOpen) void controller.reset();
    onOpenChange(nextOpen);
  }

  function changeMode(nextMode) {
    void controller.reset();
    setMode(nextMode);
  }

  async function run() {
    if (state.phase === "recording") {
      await controller.stop();
      return;
    }
    if (loading || state.busy) return;
    try {
      const snapshot = callbacks.current.getSnapshot();
      await controller.run(snapshot, mode, inputText);
      if (mode === "text" && !inputText.trim()) inputRef.current?.focus();
    } catch (error) {
      controller.fail(error);
    }
  }

  const isText = mode === "text";
  const isAsr = mode === "asr";
  const isRecording = state.phase === "recording";
  const isWaiting = state.phase === "starting" || state.phase === "processing";
  const result = state.result;
  const warnings = [...new Set([...(result?.warnings ?? []), ...(state.notice ? [state.notice] : [])])]
    .map((warning) => redactError(warning, state.snapshot));
  const action = state.phase === "starting" ? "开启麦克风…"
    : isRecording ? "停止并测试"
      : state.phase === "processing" ? isAsr ? "识别中…" : "处理中…"
        : isText ? "开始测试" : "开始录音";

  return (
    <Dialog open={open} onOpenChange={changeOpen}>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{TITLES[mode]}</DialogTitle>
          <DialogDescription className="sr-only">使用当前配置测试语音识别与文本后处理。</DialogDescription>
        </DialogHeader>

        <div className="space-y-5 py-1">
          <div className="space-y-2">
            <Label htmlFor="config-test-mode">测试方式</Label>
            <Select value={mode} onValueChange={changeMode} disabled={state.busy || loading}>
              <SelectTrigger id="config-test-mode" className="w-full"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="voice">完整语音输入</SelectItem>
                <SelectItem value="asr">仅语音识别</SelectItem>
                <SelectItem value="text">文本后处理</SelectItem>
              </SelectContent>
            </Select>
          </div>

          {isText ? (
            <div className="space-y-2">
              <Label htmlFor="config-test-text">测试文本</Label>
              <Textarea
                id="config-test-text"
                ref={inputRef}
                value={inputText}
                disabled={state.busy || loading}
                onChange={(event) => { void controller.reset(); setInputText(event.target.value); }}
                placeholder="输入要处理的文本"
                className="min-h-32 resize-y"
              />
            </div>
          ) : (
            <div className="flex items-center gap-4 rounded-xl border bg-muted/30 px-5 py-5">
              <div className={`flex size-11 shrink-0 items-center justify-center rounded-full ${isRecording ? "animate-pulse bg-foreground text-background" : "bg-muted text-muted-foreground"}`}>
                <Mic className="size-5" aria-hidden="true" />
              </div>
              <div className="flex flex-1 items-center justify-between gap-3">
                <Badge variant="secondary" aria-live="polite">{PHASES[state.phase]}</Badge>
                <span role="timer" aria-label="录音时长" className="font-mono text-xl tabular-nums tracking-tight">
                  {clockText(isRecording ? elapsed : state.recordedMs)}
                </span>
              </div>
            </div>
          )}

          {state.error ? (
            <Alert className="border-foreground/25" role="alert">
              <CircleAlert className="size-4" />
              <AlertTitle>测试未完成</AlertTitle>
              <AlertDescription className="whitespace-pre-wrap break-words">{redactError(state.error, state.snapshot)}</AlertDescription>
            </Alert>
          ) : warnings.length > 0 ? (
            <Alert>
              <CircleAlert className="size-4" />
              <AlertTitle>{state.phase === "complete" ? "测试已完成" : "录音状态"}</AlertTitle>
              <AlertDescription className="space-y-1 whitespace-pre-wrap break-words">
                {warnings.map((warning) => <p key={warning}>{warning}</p>)}
              </AlertDescription>
            </Alert>
          ) : null}

          {state.phase === "complete" && result ? (
            <div className="space-y-4" aria-live="polite">
              <div className="flex items-center justify-between gap-3 text-sm">
                <span className="flex items-center gap-2"><CircleCheck className="size-4" aria-hidden="true" />测试完成</span>
                <Badge variant="outline">{Math.round(result.elapsed_ms ?? 0)} ms</Badge>
              </div>
              {!isText ? (
                <div className="space-y-2">
                  <Label htmlFor="config-test-raw">{isAsr ? "识别结果" : "识别文本"}</Label>
                  <Textarea id="config-test-raw" value={result.raw_transcript ?? ""} readOnly className="min-h-28 resize-y bg-muted/20" />
                </div>
              ) : null}
              {!isAsr ? (
                <div className="space-y-2">
                  <Label htmlFor="config-test-final">{isText ? "后处理结果" : "最终输出"}</Label>
                  <Textarea id="config-test-final" value={result.final_text ?? ""} readOnly className="min-h-28 resize-y bg-muted/20" />
                </div>
              ) : null}
            </div>
          ) : null}
        </div>

        <DialogFooter className="gap-2 border-t pt-4">
          {state.busy ? <Button variant="outline" onClick={() => { void controller.reset(); }}>取消测试</Button>
            : <Button variant="outline" onClick={() => changeOpen(false)}>关闭</Button>}
          <Button onClick={run} disabled={isWaiting || (loading && !isRecording)}>
            {isWaiting ? <span className="animate-spin"><LoaderCircle className="size-4" aria-hidden="true" /></span>
              : isRecording ? <Square className="size-4" aria-hidden="true" />
                : isText ? <Play className="size-4" aria-hidden="true" /> : <Mic className="size-4" aria-hidden="true" />}
            {action}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
