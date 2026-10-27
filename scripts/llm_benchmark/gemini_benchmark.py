#!/usr/bin/env python3
"""Compare Gemini models on ingredient extraction from recipe images.

For every (model, image) pair, sends the image with the extraction prompt and
JSON schema, then reports cost, latency and accuracy against the expected
ingredients in scripts/llm_benchmark/expected/<image stem>.json.

Usage:
    python3 scripts/llm_benchmark/gemini_benchmark.py --list-models
    python3 scripts/llm_benchmark/gemini_benchmark.py [--models m1,m2] [--images DIR]

The API key is read from GEMINI_API_KEY (environment or .env). Only the
standard library is used.
"""

import argparse
import base64
import difflib
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request
from datetime import datetime
from pathlib import Path

API_BASE = "https://generativelanguage.googleapis.com/v1beta"
ROOT = Path(__file__).resolve().parents[2]
EXPECTED_DIR = Path(__file__).resolve().parent / "expected"
RESULTS_DIR = ROOT / "target" / "llm-benchmark"

# USD per 1M tokens (input, output incl. thinking), standard tier.
# Source: ai.google.dev/gemini-api/docs/pricing, read 2026-10-26. Check before deciding.
PRICING = {
    "gemini-3.8-flash": (0.75, 3.75),
    "gemini-3.6-flash": (0.75, 3.75),
    "gemini-3.5-flash-lite": (0.30, 2.50),
    "gemini-3.1-flash-lite": (0.25, 1.50),
    "gemini-2.5-flash-lite": (0.10, 0.40),
}

MIME_TYPES = {".jpg": "image/jpeg", ".jpeg": "image/jpeg", ".png": "image/png", ".webp": "image/webp"}

EXCLUDED_MODEL_WORDS = ("image", "tts", "live", "audio", "embedding", "exp", "8b", "robotics", "computer")

# Prompt and response schema shared with the bot (src/extraction/prompt.rs).
PROMPT = (ROOT / "config" / "extraction" / "prompt.md").read_text(encoding="utf-8")
SCHEMA = json.loads((ROOT / "config" / "extraction" / "schema.json").read_text(encoding="utf-8"))


def load_api_key():
    key = os.environ.get("GEMINI_API_KEY")
    env_file = ROOT / ".env"
    if not key and env_file.exists():
        for line in env_file.read_text().splitlines():
            if line.startswith("GEMINI_API_KEY="):
                key = line.split("=", 1)[1].strip().strip('"').strip("'")
    if not key:
        sys.exit("GEMINI_API_KEY not found in the environment or in .env")
    return key


def api_request(key, method, path, body=None, timeout=180):
    data = json.dumps(body).encode() if body is not None else None
    request = urllib.request.Request(
        f"{API_BASE}/{path}",
        data=data,
        method=method,
        headers={"x-goog-api-key": key, "Content-Type": "application/json"},
    )
    for attempt in range(4):
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            detail = error.read().decode(errors="replace")[:500]
            if error.code in (429, 500, 503) and attempt < 3:
                time.sleep(2 ** (attempt + 2))
                continue
            raise RuntimeError(f"HTTP {error.code}: {detail}") from None


def list_models(key):
    models, page_token = [], None
    while True:
        path = "models?pageSize=200" + (f"&pageToken={page_token}" if page_token else "")
        page = api_request(key, "GET", path)
        models.extend(page.get("models", []))
        page_token = page.get("nextPageToken")
        if not page_token:
            return models


def candidate_models(key):
    names = []
    for model in list_models(key):
        name = model["name"].removeprefix("models/")
        if "generateContent" not in model.get("supportedGenerationMethods", []):
            continue
        if "flash" in name and not any(word in name for word in EXCLUDED_MODEL_WORDS):
            names.append(name)
    return sorted(names)


def price_for(model):
    matches = [prefix for prefix in PRICING if model.startswith(prefix)]
    return PRICING[max(matches, key=len)] if matches else None


def thinking_config(model, level):
    if model.startswith("gemini-2.5"):
        return {"thinkingBudget": 0}
    if model.startswith("gemini-3.1-flash-lite") and level not in ("minimal", "high"):
        return {"thinkingLevel": "minimal"}
    return {"thinkingLevel": level}


def extract(key, model, image_path, thinking, media_resolution):
    generation_config = {
        "responseMimeType": "application/json",
        "responseJsonSchema": SCHEMA,
        "thinkingConfig": thinking_config(model, thinking),
    }
    if media_resolution:
        generation_config["mediaResolution"] = f"MEDIA_RESOLUTION_{media_resolution.upper()}"
    body = {
        "contents": [{
            "role": "user",
            "parts": [
                {"inlineData": {
                    "mimeType": MIME_TYPES[image_path.suffix.lower()],
                    "data": base64.b64encode(image_path.read_bytes()).decode(),
                }},
                {"text": PROMPT},
            ],
        }],
        "generationConfig": generation_config,
    }
    started = time.monotonic()
    try:
        response = api_request(key, "POST", f"models/{model}:generateContent", body)
    except RuntimeError as error:
        # Some models reject the lowest thinking levels: retry one level up.
        levels = ["minimal", "low", "medium", "high"]
        if "Thinking level" not in str(error) or thinking not in levels[:-1]:
            raise
        return extract(key, model, image_path, levels[levels.index(thinking) + 1], media_resolution)
    latency = time.monotonic() - started

    usage = response.get("usageMetadata", {})
    candidate = (response.get("candidates") or [{}])[0]
    text = "".join(part.get("text", "") for part in candidate.get("content", {}).get("parts", []) if not part.get("thought"))
    try:
        parsed = json.loads(text)
    except json.JSONDecodeError:
        parsed = None
    return {
        "latency_s": latency,
        "input_tokens": usage.get("promptTokenCount", 0),
        "output_tokens": usage.get("candidatesTokenCount", 0) + usage.get("thoughtsTokenCount", 0),
        "thought_tokens": usage.get("thoughtsTokenCount", 0),
        "finish_reason": candidate.get("finishReason"),
        "output": parsed,
        "raw_text": None if parsed is not None else text,
    }


def normalize_name(name):
    name = (name or "").lower().strip()
    return re.sub(r"[^\w\s'-]", "", name)


def score(expected, actual):
    """Return (found, quantity_ok, unit_ok, extra) against the expected ingredients."""
    remaining = list(actual)
    found = quantity_ok = unit_ok = 0
    for item in expected:
        best, best_ratio = None, 0.0
        for candidate in remaining:
            ratio = difflib.SequenceMatcher(None, normalize_name(item["name"]), normalize_name(candidate.get("name"))).ratio()
            if ratio > best_ratio:
                best, best_ratio = candidate, ratio
        if best is None or best_ratio < 0.6:
            continue
        remaining.remove(best)
        found += 1
        if item["quantity"] is None:
            quantity_ok += best.get("quantity") is None
        elif best.get("quantity") is not None:
            quantity_ok += abs(float(best["quantity"]) - item["quantity"]) < 1e-6
        unit_ok += (item["unit"] or None) == ((best.get("unit") or "").lower().strip() or None)
    return found, quantity_ok, unit_ok, len(remaining)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--list-models", action="store_true", help="list candidate Flash models and exit")
    parser.add_argument("--models", help="comma-separated model ids (default: all candidate Flash models)")
    parser.add_argument("--images", default=str(ROOT / "test_images"), help="directory of images to test")
    parser.add_argument("--thinking", default="minimal", help="Gemini 3 thinking level (minimal, low, medium, high)")
    parser.add_argument("--media-resolution", choices=["low", "medium", "high"], help="image resolution sent to the model")
    args = parser.parse_args()

    key = load_api_key()
    if args.list_models:
        for name in candidate_models(key):
            price = price_for(name)
            print(f"{name:45} {'%.2f / %.2f $/M' % price if price else 'price unknown'}")
        return

    models = args.models.split(",") if args.models else candidate_models(key)
    images = sorted(p for p in Path(args.images).iterdir() if p.suffix.lower() in MIME_TYPES)
    if not images:
        sys.exit(f"no images in {args.images}")

    run_dir = RESULTS_DIR / datetime.now().strftime("%Y%m%d-%H%M%S")
    run_dir.mkdir(parents=True, exist_ok=True)
    summary = []

    for model in models:
        totals = {"cost": 0.0, "latency": 0.0, "expected": 0, "found": 0, "quantity": 0, "unit": 0, "extra": 0, "errors": 0}
        priced = price_for(model)
        for image in images:
            label = f"{model} / {image.name}"
            try:
                result = extract(key, model, image, args.thinking, args.media_resolution)
            except RuntimeError as error:
                print(f"  ✗ {label}: {error}")
                totals["errors"] += 1
                continue
            (run_dir / f"{model}__{image.stem}.json").write_text(json.dumps(result, indent=2, ensure_ascii=False))
            if priced:
                totals["cost"] += (result["input_tokens"] * priced[0] + result["output_tokens"] * priced[1]) / 1e6
            totals["latency"] += result["latency_s"]
            expected_file = EXPECTED_DIR / f"{image.stem}.json"
            if result["output"] is None:
                totals["errors"] += 1
                print(f"  ✗ {label}: invalid JSON (finish reason {result['finish_reason']})")
            elif expected_file.exists():
                expected = json.loads(expected_file.read_text())["ingredients"]
                found, quantity, unit, extra = score(expected, result["output"].get("ingredients", []))
                for field, value in (("expected", len(expected)), ("found", found), ("quantity", quantity), ("unit", unit), ("extra", extra)):
                    totals[field] += value
            print(f"  ✓ {label}: {result['input_tokens']} in / {result['output_tokens']} out ({result['thought_tokens']} thinking), {result['latency_s']:.1f}s")
        runs = len(images) - totals["errors"]
        summary.append((model, priced, totals, runs))

    print(f"\nResults ({len(images)} images, thinking={args.thinking}, media_resolution={args.media_resolution or 'default'}) saved in {run_dir}\n")
    print(f"{'model':40} {'$/photo':>9} {'$/1000':>8} {'latency':>8} {'found':>7} {'qty ok':>7} {'unit ok':>8} {'extra':>6} {'errors':>7}")
    for model, priced, totals, runs in summary:
        per_photo = totals["cost"] / runs if runs and priced else None
        expected = totals["expected"] or 1
        print(
            f"{model:40} "
            f"{('%.5f' % per_photo) if per_photo is not None else '?':>9} "
            f"{('%.2f' % (per_photo * 1000)) if per_photo is not None else '?':>8} "
            f"{(totals['latency'] / runs if runs else 0):>7.1f}s "
            f"{totals['found'] / expected:>7.0%} {totals['quantity'] / expected:>7.0%} {totals['unit'] / expected:>8.0%} "
            f"{totals['extra']:>6} {totals['errors']:>7}"
        )


if __name__ == "__main__":
    main()
