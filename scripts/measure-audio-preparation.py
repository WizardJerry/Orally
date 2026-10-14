"""Offline preparation benchmark for the generated synthetic PCM16 WAV fixture.

Run with Python 3.12 to measure the audioop.ratecv implementation used in the
earlier provider probe. No configuration, credentials, network or microphone is
used. SciPy's anti-aliasing polyphase implementation is included when installed;
neither implementation is the application's production Rust audio path.
"""

import argparse
import base64
from datetime import datetime, timezone
import hashlib
import importlib
import io
import json
import math
from pathlib import Path
import platform
import statistics
import sys
import time
import warnings
import wave


STAGES = ("resample", "wav", "base64", "json", "total")
ROOT = Path(__file__).resolve().parent.parent
FIXTURE_ROOT = ROOT / "target" / "latency-probe"


def summarize(rows):
    """Nearest-rank P95; individual measurements are retained in nanoseconds."""
    result = {}
    for stage in STAGES:
        samples = sorted(row[stage + "_ns"] for row in rows)
        result[stage] = {
            "median_ms": round(statistics.median(samples) / 1_000_000, 6),
            "p95_ms": round(samples[math.ceil(0.95 * len(samples)) - 1] / 1_000_000, 6),
            "max_ms": round(max(samples) / 1_000_000, 6),
        }
    return result


def prepare(pcm, source_rate, target_rate, converter):
    t0 = time.perf_counter_ns()
    output_pcm = converter(pcm, source_rate, target_rate)
    t1 = time.perf_counter_ns()
    with io.BytesIO() as buffer:
        with wave.open(buffer, "wb") as writer:
            writer.setnchannels(1)
            writer.setsampwidth(2)
            writer.setframerate(target_rate)
            writer.writeframes(output_pcm)
        wav = buffer.getvalue()
    t2 = time.perf_counter_ns()
    encoded = base64.b64encode(wav).decode("ascii")
    t3 = time.perf_counter_ns()
    payload = {
        "model": "qwen3-asr-flash",
        "messages": [
            {"role": "system", "content": "Language hint: zh-CN"},
            {"role": "user", "content": [{
                "type": "input_audio",
                "input_audio": {"data": "data:audio/wav;base64," + encoded},
            }]},
        ],
        "stream": False,
    }
    body = json.dumps(payload, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    t4 = time.perf_counter_ns()
    durations = {
        "resample_ns": t1 - t0,
        "wav_ns": t2 - t1,
        "base64_ns": t3 - t2,
        "json_ns": t4 - t3,
        "total_ns": t4 - t0,
    }
    sizes = {
        "pcm_bytes": len(output_pcm),
        "wav_bytes": len(wav),
        "base64_ascii_bytes": len(encoded),
        "request_json_utf8_bytes": len(body),
        "frames": len(output_pcm) // 2,
        "duration_seconds": len(output_pcm) / (2 * target_rate),
    }
    return durations, sizes


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--audio", type=Path, default=FIXTURE_ROOT / "synthetic-48000.wav")
    parser.add_argument("--output", type=Path, default=FIXTURE_ROOT / "audio-preparation.json")
    parser.add_argument("--runs", type=int, default=50)
    parser.add_argument("--warmup", type=int, default=5)
    args = parser.parse_args()
    if args.runs < 30 or args.warmup < 1:
        parser.error("Use at least 30 measured runs and one warmup")
    audio_path = args.audio.resolve()
    if (not audio_path.is_relative_to(FIXTURE_ROOT.resolve())
            or not audio_path.name.startswith("synthetic-")
            or audio_path.suffix.lower() != ".wav"):
        parser.error("Only generated target/latency-probe/synthetic-*.wav fixtures are accepted")
    file_bytes = audio_path.read_bytes()
    with wave.open(io.BytesIO(file_bytes), "rb") as reader:
        if reader.getnchannels() != 1 or reader.getsampwidth() != 2 or reader.getcomptype() != "NONE":
            parser.error("Input must be uncompressed mono PCM16 WAV")
        source_rate = reader.getframerate()
        frames = reader.getnframes()
        pcm = reader.readframes(frames)
    if source_rate != 48000:
        parser.error("This comparison requires the 48 kHz synthetic fixture")

    methods = [("passthrough_48k", source_rate, lambda data, src, dst: data)]
    notes = {}
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", DeprecationWarning)
        try:
            audioop = importlib.import_module("audioop")
        except ImportError:
            notes["audioop"] = "unavailable; use Python 3.12 to reproduce the earlier probe"
        else:
            methods.append(("audioop_ratecv_16k", 16000,
                            lambda data, src, dst: audioop.ratecv(data, 2, 1, src, dst, None)[0]))
            notes["audioop"] = (
                "ratecv defaults weightA=1, weightB=0; simple interpolation/decimation, "
                "not a production anti-aliasing quality reference; removed in Python 3.13"
            )

    versions = {"python": sys.version, "platform": platform.platform(), "implementation": platform.python_implementation()}
    try:
        np = importlib.import_module("numpy")
        scipy = importlib.import_module("scipy")
        signal = importlib.import_module("scipy.signal")
    except ImportError:
        notes["scipy"] = "not installed; skipped without installing dependencies"
    else:
        versions.update(numpy=np.__version__, scipy=scipy.__version__)

        def polyphase(data, src, dst):
            samples = np.frombuffer(data, dtype="<i2").astype(np.float32)
            divisor = math.gcd(src, dst)
            resampled = signal.resample_poly(samples, dst // divisor, src // divisor,
                                            window=("kaiser", 5.0), padtype="constant")
            return np.clip(np.rint(resampled), -32768, 32767).astype("<i2").tobytes()

        methods.append(("scipy_polyphase_16k", 16000, polyphase))
        notes["scipy"] = (
            "resample_poly with anti-aliasing FIR, Kaiser beta=5.0 and constant padding; "
            "timing includes PCM-to-float32, filter creation, resampling, rounding, clipping and PCM16 encoding"
        )

    report = {
        "timestamp_utc": datetime.now(timezone.utc).isoformat(),
        "benchmark": "offline_synthetic_audio_request_preparation",
        "runtime": versions,
        "input": {"fixture": audio_path.name, "sha256": hashlib.sha256(file_bytes).hexdigest(),
                  "file_bytes": len(file_bytes), "pcm_bytes": len(pcm),
                  "sample_rate_hz": source_rate, "channels": 1, "bits_per_sample": 16,
                  "frames": frames, "duration_seconds": frames / source_rate},
        "method_notes": notes,
        "runs_per_method": args.runs,
        "warmup_runs_per_method": args.warmup,
        "percentile_method": "nearest_rank",
        "timing_scope": {
            "resample": "in-memory PCM input to PCM16 bytes, including conversion and allocation",
            "wav": "in-memory wave writer and output buffer allocation",
            "base64": "base64 encode and ASCII decode to Python string",
            "json": "data URL concatenation, payload construction, compact JSON serialization and UTF-8 encoding",
            "total": "sum of the four contiguous stages; wall-clock time",
            "excluded": ["disk input and WAV decode", "module import", "network", "ASR", "LLM", "text insertion", "result metadata", "final buffer destruction after function return"],
        },
        "limitations": ["single machine and one generated fixture",
                        "Python reference implementations, not production Rust timings",
                        "first_run is the first preparation call after imports, not cold application startup",
                        "no claim about ASR accuracy or acoustic resampling quality from this timing test"],
        "methods": {},
    }
    # Warm each method separately, then interleave measured methods to reduce
    # long sequential-block bias from changing CPU/system load.
    for name, target_rate, converter in methods:
        warmup = []
        for _ in range(args.warmup):
            row, sizes = prepare(pcm, source_rate, target_rate, converter)
            warmup.append(row)
        report["methods"][name] = {
            "target_rate_hz": target_rate, "output": sizes,
            "first_run_ms": {key[:-3] + "_ms": round(value / 1_000_000, 6) for key, value in warmup[0].items()},
            "warmup_samples_ns": warmup, "samples_ns": [],
        }
    for run in range(args.runs):
        for offset in range(len(methods)):
            name, target_rate, converter = methods[(run + offset) % len(methods)]
            row, sizes = prepare(pcm, source_rate, target_rate, converter)
            result = report["methods"][name]
            if sizes != result["output"]:
                raise RuntimeError("Output sizes changed between runs")
            result["samples_ns"].append(row)
    for result in report["methods"].values():
        result["summary"] = summarize(result["samples_ns"])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    summary = {"input": report["input"], "runs_per_method": args.runs, "warmup_runs_per_method": args.warmup,
               "methods": {name: {key: result[key] for key in ("output", "first_run_ms", "summary")}
                           for name, result in report["methods"].items()}}
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
