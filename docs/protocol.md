# Engine worker protocol (v1)

Python engines run as child processes: `python -u -m melfield_worker --family <name>`, started by the Rust supervisor (`crates/melfield-core/src/engines/`). Implementation: `workers/melfield_worker/`.

## Transport
- **stdin/stdout**, frames = 4-byte big-endian length + UTF-8 JSON object.
- A header containing `"bin": n` is followed by **one binary frame** of `n` bytes (audio as little-endian `f32`). No base64.
- **stderr** = logs → `logs/engine-<family>.log`. The worker rebinds `sys.stdout` to stderr so library prints can never corrupt framing.
- Every request carries `id`; events and the final reply echo it. Final replies have `ok`; events have `event`.

## Ops
| op | request | reply |
|---|---|---|
| `hello` | `{protocol}` | `{ok, family, version, protocol, watermark: "perth"|null}` |
| `load` | `{family, entry, recipe, artifacts:[{repo, revision, dir}], device: cuda|mps|cpu, sample_rate}` | events `{event:"progress", stage, pct}` → `{ok}` |
| `synthesize` | `{text, lang, voice:{preset}|{ref_audio, ref_text}, speed, delivery, watermark, stream}` | events `{event:"chunk", seq, sample_rate, format:"f32le", bin}` + bytes → `{ok, watermarked}` |
| `cancel` | `{target}` | no reply; the running op ends with `CANCELLED` |
| `health` | — | `{ok, loaded}` |
| `shutdown` | — | process exits |

Errors: `{ok:false, error:{code, detail}}`. Codes: `OUT_OF_MEMORY`, `DEVICE_UNAVAILABLE`, `MODEL_FILES_MISSING`, `DEPENDENCY_MISSING`, `WATERMARK_UNAVAILABLE`, `UNSUPPORTED_LANGUAGE`, `BAD_REFERENCE_AUDIO`, `CANCELLED`, `INTERNAL`. The core maps each to a human message + fix (`engines/worker.rs::map_worker_error`); `detail` only reaches the log.

## Watermark rule
- `watermark: true` is set by the core for every clone voice.
- The worker refuses up front (`WATERMARK_UNAVAILABLE`) if PerTh can't load — PerTh sets `PerthImplicitWatermarker = None` on import failure, so we check explicitly.
- Families without a built-in watermark buffer the utterance and apply PerTh before sending (one chunk). Chatterbox applies PerTh internally (`builtin_watermark = True`).
- The core rejects any clone reply without `watermarked: true`.

## Environment
Set by the supervisor: `HF_HUB_CACHE=<data>/models/hf`, `HF_HUB_OFFLINE=1`, `TRANSFORMERS_OFFLINE=1`, `PYTHONNOUSERSITE=1`, `PYTORCH_ENABLE_MPS_FALLBACK=1`, `HF_TOKEN` (only when signed in). Weights are pulled by the core into the HF cache layout before any worker starts.

## Lifecycle
Lazy start per family → `hello` → `load` (once per recipe) → `synthesize`*. Crash → restart once and retry; second crash → "needs repair" (Settings › Advanced › repair rebuilds the env from `workers/families/<family>/requirements-<backend>.txt`). Idle 10 min → `shutdown`.

## Tests
`uv run --no-project --with numpy --with pytest python -m pytest workers/tests -q` drives a real worker process (fake family) through hello/load/stream/cancel/errors/watermark refusal.
