#!/usr/bin/env python3
"""Build registry/index.json from registry/recipes/*.json + registry/voices.json.

Validates basic invariants so a broken recipe never ships:
  - every recipe has a pinned 40-char commit revision and per-file sizes
  - every voice points at an existing recipe
  - licence fields are present; personal-use recipes require an accept step

Usage: python scripts/build_registry.py [--check]
  --check  fail if index.json is out of date (used in CI)
"""
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent / "registry"
SHA_RE = re.compile(r"^[0-9a-f]{40}$")


def fail(msg: str) -> None:
    print(f"registry error: {msg}", file=sys.stderr)
    sys.exit(1)


def load_recipes() -> list:
    recipes = []
    for path in sorted((ROOT / "recipes").glob("*.json")):
        r = json.loads(path.read_text(encoding="utf-8"))
        if r.get("schema_version") != 1:
            fail(f"{path.name}: schema_version must be 1")
        if r["id"] != path.stem:
            fail(f"{path.name}: id must match file name")
        for a in r["artifacts"]:
            if not SHA_RE.match(a["revision"]):
                fail(f"{r['id']}: revision must be a full commit sha, got {a['revision']}")
            for f in a["files"]:
                if not isinstance(f.get("size"), int) or f["size"] <= 0:
                    fail(f"{r['id']}: {f['path']} needs a size")
        lic = r["license"]
        for key in ("id", "url", "commercial", "accept_required", "personal_use"):
            if key not in lic:
                fail(f"{r['id']}: license.{key} missing")
        if lic["personal_use"] and not lic["accept_required"]:
            fail(f"{r['id']}: personal-use recipes must require an accept step")
        recipes.append(r)
    return recipes


def main() -> None:
    recipes = load_recipes()
    ids = {r["id"] for r in recipes}
    voices = json.loads((ROOT / "voices.json").read_text(encoding="utf-8"))["voices"]
    seen = set()
    for v in voices:
        if v["recipe"] not in ids:
            fail(f"voice {v['id']} points at unknown recipe {v['recipe']}")
        if v["id"] in seen:
            fail(f"duplicate voice id {v['id']}")
        seen.add(v["id"])
    index = {"schema_version": 1, "recipes": recipes, "voices": voices}
    text = json.dumps(index, indent=1, ensure_ascii=False) + "\n"
    out = ROOT / "index.json"
    if "--check" in sys.argv:
        if not out.exists() or out.read_text(encoding="utf-8") != text:
            fail("index.json is out of date: run python scripts/build_registry.py")
        print("registry ok")
        return
    out.write_text(text, encoding="utf-8")
    print(f"wrote {out} ({len(recipes)} recipes, {len(voices)} voices)")


if __name__ == "__main__":
    main()
