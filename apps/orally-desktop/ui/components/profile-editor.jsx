import { useState } from "react";
import Mic from "lucide-react/dist/esm/icons/mic.mjs";
import Workflow from "lucide-react/dist/esm/icons/workflow.mjs";
import Plus from "lucide-react/dist/esm/icons/plus.mjs";
import ArrowUp from "lucide-react/dist/esm/icons/arrow-up.mjs";
import ArrowDown from "lucide-react/dist/esm/icons/arrow-down.mjs";
import Trash2 from "lucide-react/dist/esm/icons/trash-2.mjs";
import ChevronDown from "lucide-react/dist/esm/icons/chevron-down.mjs";
import Eye from "lucide-react/dist/esm/icons/eye.mjs";
import EyeOff from "lucide-react/dist/esm/icons/eye-off.mjs";
import Plug from "lucide-react/dist/esm/icons/plug.mjs";
import Check from "lucide-react/dist/esm/icons/check.mjs";
import CircleAlert from "lucide-react/dist/esm/icons/circle-alert.mjs";
import LoaderCircle from "lucide-react/dist/esm/icons/loader-circle.mjs";
import MessageSquare from "lucide-react/dist/esm/icons/message-square.mjs";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/ui/switch";
import { Separator } from "@/components/ui/separator";
import { Select, SelectTrigger, SelectValue, SelectContent, SelectItem } from "@/components/ui/select";
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from "@/components/ui/collapsible";
import { Tooltip, TooltipTrigger, TooltipContent } from "@/components/ui/tooltip";

export function IconButton({ label, children, ...props }) {
  return <Tooltip><TooltipTrigger asChild>
    <Button type="button" variant="ghost" size="icon" aria-label={label} {...props}>{children}</Button>
  </TooltipTrigger><TooltipContent>{label}</TooltipContent></Tooltip>;
}

function KeyField({ id, value, onChange, disabled }) {
  const [visible, setVisible] = useState(false);
  return <div className="space-y-2">
    <Label htmlFor={id}>API Key</Label>
    <div className="relative">
      <Input id={id} className="pr-10 font-mono" type={visible ? "text" : "password"} value={value ?? ""}
        onChange={(event) => onChange(event.target.value.trim() || null)} autoComplete="off" spellCheck={false} placeholder="输入 API Key" disabled={disabled} />
      <IconButton label={visible ? "隐藏 API Key" : "显示 API Key"} className="absolute right-0.5 top-0.5 size-8" aria-pressed={visible}
        onClick={() => setVisible((current) => !current)} disabled={disabled}>{visible ? <EyeOff /> : <Eye />}</IconButton>
    </div>
  </div>;
}

function ServiceFields({ prefix, service, onChange, disabled }) {
  return <div className="grid gap-5 sm:grid-cols-2">
    <div className="space-y-2 sm:col-span-2">
      <Label htmlFor={`${prefix}-base-url`}>服务地址</Label>
      <Input id={`${prefix}-base-url`} value={service.base_url} onChange={(event) => onChange("base_url", event.target.value)}
        placeholder="https://api.example.com/v1" spellCheck={false} autoComplete="off" disabled={disabled} />
    </div>
    <div className="space-y-2">
      <Label htmlFor={`${prefix}-model`}>模型名称</Label>
      <Input id={`${prefix}-model`} value={service.model} onChange={(event) => onChange("model", event.target.value)}
        placeholder="输入模型名称" spellCheck={false} autoComplete="off" disabled={disabled} />
    </div>
    <KeyField id={`${prefix}-api-key`} value={service.api_key} onChange={(value) => onChange("api_key", value)} disabled={disabled} />
  </div>;
}

function ConnectionAction({ controller, modelId = null, disabled, children }) {
  const test = controller.connection(modelId);
  return <div className="flex flex-wrap items-center justify-between gap-3">
    <p className={`connection-result ${test?.error ? "font-medium text-foreground" : "text-muted-foreground"}`} role="status" aria-live="polite">
      {test?.message ? <>{test.busy ? <LoaderCircle className="animate-spin" /> : test.error ? <CircleAlert /> : <Check />}<span>{test.message}</span></> : null}
    </p>
    <div className="ml-auto flex items-center gap-2">
      {children}
      <Button type="button" variant="outline" size="sm" disabled={disabled || test?.busy} onClick={() => controller.runConnection(modelId)}>
        {test?.busy ? <LoaderCircle className="animate-spin" /> : <Plug />}{test?.busy ? "测试中…" : "测试连接"}
      </Button>
    </div>
  </div>;
}

function PromptNode({ controller, model, prompt, index, disabled }) {
  const change = (key, value) => controller.edit(() => { prompt[key] = value; }, model.id);
  return <div className={`prompt-node ${prompt.enabled ? "" : "node-disabled"}`} data-prompt-id={prompt.id}>
    <div className="flex flex-wrap items-center gap-2">
      <MessageSquare className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
      <Input className="h-8 min-w-24 flex-1 border-transparent bg-transparent shadow-none hover:border-input focus-visible:border-input"
        value={prompt.name} aria-label={`Prompt ${index + 1} 名称`} onChange={(event) => change("name", event.target.value)} disabled={disabled} />
      <Switch checked={prompt.enabled} onCheckedChange={(value) => change("enabled", value)} aria-label={`启用 ${prompt.name}`} disabled={disabled} />
      <IconButton label="上移 Prompt" className="size-8" disabled={disabled || index === 0} onClick={() => controller.edit((editor) => editor.movePrompt(model.id, prompt.id, -1), model.id)}><ArrowUp /></IconButton>
      <IconButton label="下移 Prompt" className="size-8" disabled={disabled || index === model.prompts.length - 1} onClick={() => controller.edit((editor) => editor.movePrompt(model.id, prompt.id, 1), model.id)}><ArrowDown /></IconButton>
      <IconButton label="删除 Prompt" className="size-8" disabled={disabled} onClick={() => controller.edit((editor) => editor.removePrompt(model.id, prompt.id), model.id)}><Trash2 /></IconButton>
    </div>
    <Textarea value={prompt.content} aria-label={`${prompt.name} 内容`} rows={3} className="mt-3 min-h-24 resize-y bg-background"
      onChange={(event) => change("content", event.target.value)} placeholder="输入这一步的提示词…" disabled={disabled} />
  </div>;
}

function ModelNode({ controller, model, index, total, disabled }) {
  const [open, setOpen] = useState(true);
  const change = (key, value) => controller.edit(() => { model[key] = value; }, model.id);
  return <Collapsible open={open} onOpenChange={setOpen} className={`model-node ${model.enabled ? "" : "node-disabled"}`} data-model-id={model.id}>
    <div className="model-node-header">
      <CollapsibleTrigger asChild>
        <Button variant="ghost" size="icon" className="size-8 shrink-0" aria-label={`${open ? "收起" : "展开"}${model.name}`}>
          <ChevronDown className={`transition-transform ${open ? "" : "-rotate-90"}`} />
        </Button>
      </CollapsibleTrigger>
      <Badge variant="outline" className="shrink-0 font-mono tabular-nums">{String(index + 1).padStart(2, "0")}</Badge>
      <div className="min-w-0 flex-1">
        <Input value={model.name} onChange={(event) => change("name", event.target.value)} aria-label={`模型 ${index + 1} 名称`}
          className="h-8 border-transparent bg-transparent font-medium shadow-none hover:border-input focus-visible:border-input" disabled={disabled} />
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <Switch className="mr-2" checked={model.enabled} onCheckedChange={(value) => change("enabled", value)} aria-label={`启用 ${model.name}`} disabled={disabled} />
        <IconButton label="上移模型" className="size-8" disabled={disabled || index === 0} onClick={() => controller.edit((editor) => editor.moveModel(model.id, -1))}><ArrowUp /></IconButton>
        <IconButton label="下移模型" className="size-8" disabled={disabled || index === total - 1} onClick={() => controller.edit((editor) => editor.moveModel(model.id, 1))}><ArrowDown /></IconButton>
        <IconButton label={total === 1 ? "至少保留一个模型，可关闭启用开关" : "删除模型"} className="size-8" disabled={disabled || total === 1}
          onClick={() => controller.edit((editor) => editor.removeModel(model.id), model.id)}><Trash2 /></IconButton>
      </div>
    </div>
    <CollapsibleContent>
      <div className="space-y-5 border-t p-5">
        <ServiceFields prefix={model.id} service={model} onChange={change} disabled={disabled} />
        <div className="space-y-2">
          <Label htmlFor={`${model.id}-prompt`}>Prompt</Label>
          <Textarea id={`${model.id}-prompt`} value={model.system_prompt} onChange={(event) => change("system_prompt", event.target.value)}
            rows={4} className="min-h-28 resize-y leading-6" placeholder="输入模型的提示词…" disabled={disabled} />
        </div>
        <ConnectionAction controller={controller} modelId={model.id} disabled={disabled} />
        <Separator />
        {model.prompts.map((prompt, promptIndex) => <PromptNode key={prompt.id} controller={controller} model={model} prompt={prompt} index={promptIndex} disabled={disabled} />)}
        <Button variant="ghost" size="sm" className="text-muted-foreground" disabled={disabled}
          onClick={() => controller.edit((editor) => editor.addPrompt(model.id), model.id)}><Plus />添加 Prompt 节点</Button>
      </div>
    </CollapsibleContent>
  </Collapsible>;
}

export function ProfileEditor({ controller, editor, disabled, onTest, saving }) {
  const profile = editor.activeProfile;
  const models = profile.postprocess.models;
  const enabledCount = models.filter((model) => model.enabled).length;
  return <div className="profile-editor">
    <div className="profile-heading">
      <div className="space-y-2">
        <Label htmlFor="config-name" className="text-xs text-muted-foreground">配置名称</Label>
        <Input id="config-name" value={profile.name} maxLength={80} onChange={(event) => controller.edit(() => { profile.name = event.target.value; })}
          className="profile-name" placeholder="为这套配置命名" autoComplete="off" disabled={disabled} />
      </div>
      <Badge variant="outline" className="gap-1.5 self-end whitespace-nowrap" role="status">
        {saving ? <LoaderCircle className="size-3 animate-spin" /> : editor.dirty ? <span className="size-1.5 rounded-full bg-current" /> : <Check className="size-3" />}
        {saving ? "保存中" : editor.dirty ? "未保存" : "已保存"}
      </Badge>
    </div>

    <Card className="stage-card">
      <CardHeader className="stage-heading">
        <div className="stage-icon"><Mic /></div>
        <CardTitle className="text-base">语音识别</CardTitle>
        <span className="ml-auto font-mono text-xs text-muted-foreground">01 / 识别</span>
      </CardHeader>
      <CardContent className="space-y-5">
        <ServiceFields prefix="asr" service={profile.asr} disabled={disabled}
          onChange={(key, value) => controller.edit(() => { profile.asr[key] = value; }, null)} />
        <ConnectionAction controller={controller} disabled={disabled}>
          <Button variant="ghost" size="sm" disabled={disabled || saving} onClick={() => onTest("asr")}><Mic />录音测试</Button>
        </ConnectionAction>
      </CardContent>
    </Card>

    <div className="stage-connector" aria-hidden="true"><ArrowDown /></div>

    <Card className="stage-card">
      <CardHeader className="stage-heading">
        <div className="stage-icon"><Workflow /></div>
        <CardTitle className="text-base">后处理</CardTitle>
        <span className="ml-auto font-mono text-xs text-muted-foreground">02 / 整理</span>
      </CardHeader>
      <CardContent className="space-y-5">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <Label htmlFor="postprocess-mode">处理方式</Label>
          <Select value={profile.postprocess.mode} disabled={disabled} onValueChange={(value) => controller.edit(() => { profile.postprocess.mode = value; })}>
            <SelectTrigger id="postprocess-mode" className="w-44"><SelectValue /></SelectTrigger>
            <SelectContent><SelectItem value="llm">AI 模型处理</SelectItem><SelectItem value="builtin">本地基础清理</SelectItem></SelectContent>
          </Select>
        </div>
        {profile.postprocess.mode === "builtin" ? <p className="rounded-md border border-dashed bg-muted/40 px-4 py-3 text-sm text-muted-foreground">使用本地基础清理。下方模型配置已保留。</p> : null}
        <div className="space-y-4" aria-label="后处理模型节点">
          {models.map((model, index) => <ModelNode key={model.id} controller={controller} model={model} index={index} total={models.length} disabled={disabled} />)}
        </div>
        <Button variant="outline" className="w-full border-dashed" disabled={disabled} onClick={() => controller.edit((current) => current.addModel())}><Plus />添加模型节点</Button>
        <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
          <span>{profile.postprocess.mode === "llm" ? `${enabledCount} 个模型已启用` : "本地处理"}</span>
          <span>{models.reduce((count, model) => count + model.prompts.length, 0)} 个 Prompt 节点</span>
        </div>
      </CardContent>
    </Card>
  </div>;
}
