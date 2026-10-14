"""Content-free, opt-in live provider benchmark. Uses only synthetic test input.

Reads an explicit profile without migrating or changing user configuration.
Requires Python 3.11+ and requests. Credentials and response bodies are never saved.
Exit 1 means a request failed or exceeded --budget-ms; this is not a CI unit test.
The Rust latency_probe example measures the actual application pipeline.
"""

import argparse
import base64
import json
import os
from pathlib import Path
import time
import tomllib
from urllib.parse import urlsplit, urlunsplit

import requests


TEST_TEXT = (
    "今天我们测试语音输入的响应速度。用户说完一段话以后，希望系统尽快把准确的文字输入到当前窗口。"
    "第一步检查录音文件的长度和大小，第二步测量语音识别服务的处理时间，第三步测量文字整理服务的响应时间。"
    "测试过程中使用固定的公开文本，不包含真实用户的私人信息。我们会分别记录每个请求的开始时间和结束时间，"
    "比较不同方案的速度和识别结果。最终目标是让日常输入更加流畅，同时保持原有的意思，正确处理中文标点和常见的技术词汇。"
)

FIXTURE_KEYWORDS = (
    "语音输入", "当前窗口", "录音文件", "语音识别", "文字整理", "公开文本",
    "私人信息", "开始时间", "结束时间", "原有的意思", "中文标点", "技术词汇",
)


def fixture_check(text):
    """Content-free Levenshtein and keyword checks for the fixed synthetic fixture."""
    reference = "".join(char.lower() for char in TEST_TEXT if char.isalnum())
    actual = "".join(char.lower() for char in text if char.isalnum())
    previous = list(range(len(actual) + 1))
    for index, expected in enumerate(reference, 1):
        current = [index]
        for offset, observed in enumerate(actual, 1):
            current.append(min(
                current[-1] + 1, previous[offset] + 1,
                previous[offset - 1] + (expected != observed),
            ))
        previous = current
    return {
        "reference_chars": len(reference), "normalized_output_chars": len(actual),
        "character_similarity": round(1 - previous[-1] / max(len(reference), len(actual), 1), 4),
        "keyword_retention_rate": round(sum(word in actual for word in FIXTURE_KEYWORDS) / len(FIXTURE_KEYWORDS), 4),
        "exact_normalized_match": reference == actual,
        "metric_scope": "fixed_synthetic_fixture_only_not_an_accuracy_guarantee",
    }


def pipeline_connection(config, parser):
    key = config.get("api_key") or os.environ.get(config.get("api_key_env", ""))
    if not key:
        parser.error("No provider credential available")
    parts = urlsplit(config["base_url"].strip())
    path = parts.path.rstrip("/")
    for suffix in ["/audio/transcriptions", "/chat/completions"]:
        if path.endswith(suffix):
            path = path[:-len(suffix)]
            break
    url = urlunsplit((parts.scheme, parts.netloc, path + "/chat/completions", parts.query, ""))
    return url, {"Authorization": "Bearer " + key, "Content-Type": "application/json"}


def pipeline_send(session, connection, payload):
    """Keep provider text in memory; return only allowlisted metadata to the report."""
    body = json.dumps(payload, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    url, headers = connection
    row, content = {"request_bytes": len(body)}, ""
    started = time.perf_counter()
    try:
        with session.post(url, headers=headers, data=body, timeout=(10, 25), stream=True) as response:
            row["headers_ms"] = round((time.perf_counter() - started) * 1000)
            row["status"] = response.status_code
            data = response.json()
            row["elapsed_ms"] = round((time.perf_counter() - started) * 1000)
            if not isinstance(data, dict):
                raise ValueError("Unexpected response shape")
            choices = data.get("choices")
            choice = choices[0] if isinstance(choices, list) and choices else {}
            choice = choice if isinstance(choice, dict) else {}
            message = choice.get("message")
            message = message if isinstance(message, dict) else {}
            content = message.get("content")
            content = content if isinstance(content, str) else ""
            reasoning = message.get("reasoning_content")
            row["output_chars"] = len(content)
            row["reasoning_chars"] = len(reasoning) if isinstance(reasoning, str) else 0
            reason = choice.get("finish_reason")
            row["finish_reason"] = reason if reason in (
                None, "stop", "length", "content_filter", "tool_calls", "function_call",
            ) else "other"
            usage = data.get("usage")
            usage = usage if isinstance(usage, dict) else {}
            row["usage"] = {
                key: value for key, value in usage.items()
                if key in ("prompt_tokens", "completion_tokens", "total_tokens")
                and isinstance(value, (int, float)) and not isinstance(value, bool)
            }
            details = usage.get("completion_tokens_details")
            details = details if isinstance(details, dict) else {}
            if isinstance(details.get("reasoning_tokens"), (int, float)):
                row["usage"]["reasoning_tokens"] = details["reasoning_tokens"]
            row["success"] = response.ok and bool(content.strip())
    except (requests.RequestException, ValueError) as error:
        row.update(elapsed_ms=round((time.perf_counter() - started) * 1000),
                   success=False, error_type=type(error).__name__)
    return row, content


def run_pipeline(args, profile, parser):
    if not args.audio:
        parser.error("--audio is required for pipeline")
    post = profile["postprocess"]
    models = [model for model in post.get("models", []) if model.get("enabled", True)]
    if len(models) > 1:
        parser.error("Use Rust latency_probe for a multi-model pipeline")
    if post.get("mode") not in ("llm", "ai") or (post.get("models") and not models):
        parser.error("Pipeline requires an enabled AI refinement model")
    configs = {"asr": profile["asr"], "llm": models[0] if models else post}
    connections = {stage: pipeline_connection(config, parser) for stage, config in configs.items()}
    audio = base64.b64encode(args.audio.read_bytes()).decode("ascii")
    messages = []
    if configs["asr"].get("language"):
        messages.append({"role": "system", "content": "Language hint: " + configs["asr"]["language"]})
    messages.append({"role": "user", "content": [{"type": "input_audio", "input_audio": {"data": "data:audio/wav;base64," + audio}}]})
    asr_payload = {"model": configs["asr"]["model"], "messages": messages, "stream": False}
    llm_config = configs["llm"]
    prompt = "\n\n".join(
        part for part in [llm_config.get("system_prompt", "")]
        + [item["content"] for item in llm_config.get("prompts", []) if item.get("enabled", True)]
        if part.strip()
    )
    rows, sessions = [], {}
    try:
        for run in range(1, args.runs + 1):
            started = time.perf_counter()
            # Each provider has its own client. --reuse only changes cross-run lifetime.
            for stage in ("asr", "llm"):
                if not args.reuse or stage not in sessions:
                    if stage in sessions:
                        sessions[stage].close()
                    sessions[stage] = requests.Session()
            asr, transcript = pipeline_send(sessions["asr"], connections["asr"], asr_payload)
            llm, final_text = None, ""
            if asr["success"]:
                user = llm_config["user_template"].replace("{{transcript}}", transcript).replace("{{locale}}", args.locale)
                payload = {"model": llm_config["model"], "messages": [{"role": "system", "content": prompt}, {"role": "user", "content": user}], "temperature": 0.1, "stream": False}
                if args.thinking == "off":
                    payload["enable_thinking"] = False
                llm, final_text = pipeline_send(sessions["llm"], connections["llm"], payload)
            processing_ms = round((time.perf_counter() - started) * 1000)
            success = asr["success"] and llm is not None and llm["success"]
            row = {
                "run": run, "stage": "pipeline", "thinking": args.thinking, "reuse": args.reuse,
                "asr_model": configs["asr"]["model"], "llm_model": configs["llm"]["model"],
                "asr_protocol": "chat_audio_data_url", "provider_sessions": "separate_asr_and_llm",
                "processing_ms": processing_ms, "elapsed_ms": processing_ms,
                "asr_ms": asr["elapsed_ms"], "llm_ms": llm["elapsed_ms"] if llm else None,
                "asr": asr, "llm": llm, "success": success, "budget_ms": args.budget_ms,
                "within_budget": bool(success and processing_ms < args.budget_ms),
                "processing_boundary": "requests_client_setup_and_llm_template_rendering",
                "excluded": "recording_wav_preparation_protocol_negotiation_insertion_fixture_check",
            }
            if args.check_fixture:
                row["asr_fixture_check"] = fixture_check(transcript)
                row["final_fixture_check"] = fixture_check(final_text)
            rows.append(row)
            print(json.dumps(row, ensure_ascii=False), flush=True)
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(rows, ensure_ascii=False, indent=2), encoding="utf-8")
    finally:
        for session in sessions.values():
            session.close()
    return 0 if rows and all(row["within_budget"] for row in rows) else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", type=Path, required=True)
    parser.add_argument("--stage", choices=["asr", "llm", "pipeline"], required=True)
    parser.add_argument("--audio", type=Path, help="Synthetic WAV only; required for ASR/pipeline")
    parser.add_argument("--thinking", choices=["default", "off"], default="default")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--reuse", action="store_true")
    parser.add_argument("--locale", default="zh-CN")
    parser.add_argument("--check-fixture", action="store_true", help="Assert audio contains TEST_TEXT; output only similarity and keyword retention, not an accuracy guarantee")
    parser.add_argument("--budget-ms", type=int, default=5000)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.runs < 1 or args.budget_ms < 1:
        parser.error("--runs and --budget-ms must be positive")
    profile = tomllib.loads(args.profile.read_text(encoding="utf-8-sig"))
    if args.stage == "pipeline":
        return run_pipeline(args, profile, parser)
    if args.stage == "asr":
        if not args.audio:
            parser.error("--audio is required for ASR")
        config = profile["asr"]
        audio = base64.b64encode(args.audio.read_bytes()).decode("ascii")
        messages = []
        if config.get("language"):
            messages.append({"role": "system", "content": "Language hint: " + config["language"]})
        messages.append({"role": "user", "content": [{"type": "input_audio", "input_audio": {"data": "data:audio/wav;base64," + audio}}]})
        payload = {"model": config["model"], "messages": messages, "stream": False}
    else:
        post = profile["postprocess"]
        models = [m for m in post.get("models", []) if m.get("enabled", True)]
        if len(models) > 1:
            parser.error("Use Rust latency_probe for a multi-model pipeline")
        config = models[0] if models else post
        prompt = "\n\n".join(p for p in [config.get("system_prompt", "")] + [p["content"] for p in config.get("prompts", []) if p.get("enabled", True)] if p.strip())
        user = config["user_template"].replace("{{transcript}}", TEST_TEXT).replace("{{locale}}", args.locale)
        payload = {"model": config["model"], "messages": [{"role": "system", "content": prompt}, {"role": "user", "content": user}], "temperature": 0.1, "stream": False}
        if args.thinking == "off":
            payload["enable_thinking"] = False
    key = config.get("api_key") or os.environ.get(config.get("api_key_env", ""))
    if not key:
        parser.error("No provider credential available")
    url = config["base_url"].rstrip("/")
    for suffix in ["/audio/transcriptions", "/chat/completions"]:
        if url.endswith(suffix):
            url = url[:-len(suffix)]
            break
    url += "/chat/completions"
    body = json.dumps(payload, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    rows = []
    session = requests.Session()
    try:
        for run in range(1, args.runs + 1):
            if not args.reuse:
                session.close()
                session = requests.Session()
            row = {"run": run, "stage": args.stage, "model": config["model"], "thinking": args.thinking, "reuse": args.reuse, "request_bytes": len(body)}
            started = time.perf_counter()
            content = ""
            try:
                with session.post(url, headers={"Authorization": "Bearer " + key, "Content-Type": "application/json"}, data=body, timeout=(10, 25), stream=True) as response:
                    row["headers_ms"] = round((time.perf_counter() - started) * 1000)
                    row["status"] = response.status_code
                    data = response.json()
                    row["elapsed_ms"] = round((time.perf_counter() - started) * 1000)
                    choice = (data.get("choices") or [{}])[0]
                    message = choice.get("message") or {}
                    content = message.get("content") or ""
                    row["output_chars"] = len(message.get("content") or "")
                    row["reasoning_chars"] = len(message.get("reasoning_content") or "")
                    row["finish_reason"] = choice.get("finish_reason")
                    # Provider usage is copied through a numeric allowlist only.
                    usage = data.get("usage") or {}
                    row["usage"] = {k: v for k, v in usage.items() if k in ("prompt_tokens", "completion_tokens", "total_tokens") and isinstance(v, (int, float))}
                    detail = usage.get("completion_tokens_details") or {}
                    if isinstance(detail.get("reasoning_tokens"), (int, float)):
                        row["usage"]["reasoning_tokens"] = detail["reasoning_tokens"]
                    row["success"] = response.ok and row["output_chars"] > 0
            except (requests.RequestException, ValueError) as error:
                row.update(elapsed_ms=round((time.perf_counter() - started) * 1000), success=False, error_type=type(error).__name__)
            row["within_budget"] = row["success"] and row["elapsed_ms"] < args.budget_ms
            if args.check_fixture:
                row["fixture_check"] = fixture_check(content)
            rows.append(row)
            print(json.dumps(row, ensure_ascii=False), flush=True)
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(rows, ensure_ascii=False, indent=2), encoding="utf-8")
    finally:
        session.close()
    return 0 if rows and all(row["within_budget"] for row in rows) else 1


if __name__ == "__main__":
    raise SystemExit(main())
