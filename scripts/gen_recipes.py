import json, sys, os

d = json.load(open(os.path.join(os.path.dirname(__file__), "..", "trees.json")))  # from: python scripts/fetch_trees.py trees.json
out_dir = sys.argv[1]
SKIP = {'.gitattributes', 'README.md'}


def art(key, allow=None, deny_prefix=()):
    t = d[key]
    files = [f for f in t['files'] if f['path'] not in SKIP and not f['path'].endswith('.py')
             and (allow is None or any(f['path'].startswith(a) for a in allow))
             and not any(f['path'].startswith(p) for p in deny_prefix)]
    return {"source": "huggingface", "repo": t['repo'], "revision": t['revision'],
            "gated": t['gated'] not in (False, None), "files": files}


def pick(t, paths):
    return {"source": "huggingface", "repo": t['repo'], "revision": t['revision'],
            "gated": t['gated'] not in (False, None), "files": [f for f in t['files'] if f['path'] in paths]}


def tot(arts):
    return sum(f['size'] for a in arts for f in a['files'])


def lic(id_, url, commercial=True, attribution=None, policy=None, accept=False, personal=False):
    return {"id": id_, "url": url, "commercial": commercial, "attribution": attribution,
            "accept_required": accept, "personal_use": personal, "usage_policy": policy}


def caps(**k):
    base = {"clone": False, "clone_ref_seconds": {"min": 0, "ideal": 0, "max": 0}, "clone_needs_transcript": False,
            "voice_design": False, "emotion": None, "tags": [], "speed": False, "delivery": [],
            "streaming": "chunk", "max_chunk_chars": 300, "builtin_watermark": None}
    base.update(k)
    return base


def hw(devices, os_, min_ram=4, min_vram=None, macos_min=None):
    return {"devices": devices, "min_ram_gb": min_ram, "min_vram_gb": min_vram, "os": os_,
            "macos_min": macos_min, "realtime_hint": {}}


def recipe(**k):
    arts = k['artifacts']
    base = {"schema_version": 1, "download_bytes": tot(arts), "disk_bytes": tot(arts), "gate": None,
            "g2p": {}, "notes_internal": ""}
    base.update(k)
    return base


PY_OS = ["windows-x64", "linux-x64", "macos-arm64"]
ALL_OS = ["windows-x64", "macos-arm64", "macos-x64", "linux-x64"]
DELIVERY = ["calm", "natural", "expressive"]
recipes = []

kok = {"source": "huggingface", "repo": "csukuangfj/kokoro-int8-multi-lang-v1_0",
       "revision": "57adb37fc0e8d4bd430cfc1b8f8c2294945b9af2", "gated": False,
       "files": [{"path": "model.int8.onnx", "size": 114202993, "sha256": "75635ea3b85a32d00094dcdd61a258aba87c160f7c264c304a7b3ae9d734195e"},
                 {"path": "voices.bin", "size": 28200960, "sha256": "1c5a5b983d3d50d8586d437a51f3faa2da7919ce76a013c081e65671a3447c29"},
                 {"path": "tokens.txt", "size": 687, "sha256": "6ebb6bb288f20f3ae8d004d3c2ca27697da27c037d75e81a60e2a6a663f95425"}]}
recipes.append(recipe(
    id="kokoro-82m", display_name="Kokoro 82M (int8)",
    engine={"kind": "native", "runtime": "sherpa-onnx", "model_type": "kokoro"},
    artifacts=[kok],
    license=lic("Apache-2.0", "https://huggingface.co/hexgrad/Kokoro-82M",
                attribution="Kokoro-82M by hexgrad (Apache-2.0); ONNX packaging by k2-fsa/sherpa-onnx (Apache-2.0). Training data includes CC BY corpora (Koniwa, SIWIS)."),
    capabilities=caps(speed=True, max_chunk_chars=260),
    languages=["en"], g2p={"en": "misaki-rs+melfield-oov"}, sample_rate=24000,
    hardware=hw(["cpu"], ALL_OS, min_ram=2), tier="starter",
    notes_internal="Hindi/es/fr/it/pt Kokoro voices need espeak-ng G2P (GPL); not enabled (espeak-ng is GPL-3.0)."))

QL = ["zh", "en", "ja", "ko", "de", "fr", "ru", "pt", "es", "it"]
qlic = lic("Apache-2.0", "https://github.com/QwenLM/Qwen3-TTS", attribution="Qwen3-TTS by Alibaba Qwen (Apache-2.0)")
for rid, key, name, vram in [("qwen3-tts-0.6b-base", 'qwen3-0.6b-base', "Qwen3-TTS 0.6B Base", 4),
                             ("qwen3-tts-1.7b-base", 'qwen3-1.7b-base', "Qwen3-TTS 1.7B Base", 8)]:
    recipes.append(recipe(
        id=rid, display_name=name,
        engine={"kind": "python", "family": "qwen3", "entry": "base", "python": "3.12", "lock": "qwen3"},
        artifacts=[art(key)], license=qlic,
        capabilities=caps(clone=True, clone_ref_seconds={"min": 3, "ideal": 10, "max": 30}, clone_needs_transcript=True, max_chunk_chars=400),
        languages=QL, sample_rate=24000,
        hardware=hw(["cuda", "mps", "cpu"], PY_OS, min_ram=8, min_vram=vram, macos_min="14.0"), tier="standard",
        notes_internal="VRAM figures are estimates until benchmarked (not yet benchmarked)."))
recipes.append(recipe(
    id="qwen3-tts-1.7b-customvoice", display_name="Qwen3-TTS 1.7B CustomVoice",
    engine={"kind": "python", "family": "qwen3", "entry": "custom_voice", "python": "3.12", "lock": "qwen3"},
    artifacts=[art('qwen3-1.7b-customvoice')], license=qlic,
    capabilities=caps(emotion="instruction", max_chunk_chars=400, delivery=DELIVERY),
    languages=QL, sample_rate=24000,
    hardware=hw(["cuda", "mps"], PY_OS, min_ram=8, min_vram=8, macos_min="14.0"), tier="standard"))

CBL = ["ar", "da", "de", "el", "en", "es", "fi", "fr", "he", "hi", "it", "ja", "ko", "ms", "nl", "no", "pl", "pt", "ru", "sv", "sw", "tr", "zh"]
cblic = lic("MIT", "https://huggingface.co/ResembleAI/chatterbox", attribution="Chatterbox by Resemble AI (MIT). Output carries a Perth watermark.")
recipes.append(recipe(
    id="chatterbox-multilingual-v3", display_name="Chatterbox Multilingual v3",
    engine={"kind": "python", "family": "chatterbox", "entry": "multilingual_v3", "python": "3.11", "lock": "chatterbox"},
    artifacts=[art('chatterbox-ml-v3')], license=cblic,
    capabilities=caps(clone=True, clone_ref_seconds={"min": 5, "ideal": 10, "max": 30}, emotion="exaggeration", delivery=DELIVERY, builtin_watermark="perth"),
    languages=CBL, sample_rate=24000,
    hardware=hw(["cuda", "mps", "cpu"], PY_OS, min_ram=8, min_vram=4, macos_min="14.0"), tier="standard",
    notes_internal="Installed from git @ab55923c (PyPI 0.1.7 lacks v3). torch pin 2.6 lacks cu128; override tested in M4."))
recipes.append(recipe(
    id="chatterbox-turbo", display_name="Chatterbox Turbo",
    engine={"kind": "python", "family": "chatterbox", "entry": "turbo", "python": "3.11", "lock": "chatterbox"},
    artifacts=[art('chatterbox-turbo', deny_prefix=("s3gen.safetensors",))], license=cblic,
    capabilities=caps(clone=True, clone_ref_seconds={"min": 6, "ideal": 10, "max": 15},
                      tags=["[laugh]", "[chuckle]", "[sigh]", "[cough]", "[gasp]", "[groan]", "[sniff]", "[shush]", "[clear throat]",
                            "[whispering]", "[sarcastic]", "[happy]", "[crying]", "[narration]", "[dramatic]", "[advertisement]",
                            "[surprised]", "[fear]", "[angry]"],
                      builtin_watermark="perth"),
    languages=["en"], sample_rate=24000,
    hardware=hw(["cuda", "mps", "cpu"], PY_OS, min_ram=8, min_vram=4, macos_min="14.0"), tier="advanced",
    notes_internal="Reference must be longer than 5 s (assert upstream)."))

pg, pn = d['pocket-tts'], d['pocket-tts-nc']
EMB = ["alba", "jean", "eve", "george", "jane"]
pocket_nc = pick(pn, ["languages/english/model.safetensors"] + [f"languages/english/embeddings/{v}.safetensors" for v in EMB])
# pocket-tts 4.0.1's english.yaml pins its tokenizer at a different commit than the weights.
pocket_tok = {"source": "huggingface", "repo": "kyutai/pocket-tts-without-voice-cloning", "revision": "00eac05ed3d16bdc3f6b5d598874019c34a89214",
              "gated": False, "files": [{"path": "languages/english/tokenizer.json", "size": 245020, "sha256": None}]}
pocket_clone = pick(pg, ["languages/english/model.safetensors"])
plic = lic("CC-BY-4.0", "https://huggingface.co/kyutai/pocket-tts",
           attribution="Pocket TTS by Kyutai: weights CC BY 4.0, code MIT. Voice embeddings: see kyutai/tts-voices.",
           policy="Do not clone a voice without the speaker's consent.")
recipes.append(recipe(
    id="pocket-tts", display_name="Pocket TTS (English)",
    engine={"kind": "python", "family": "pocket", "entry": "english", "python": "3.12", "lock": "pocket"},
    artifacts=[pocket_nc, pocket_tok], license=plic,
    capabilities=caps(streaming="native", max_chunk_chars=500),
    languages=["en"], sample_rate=24000, hardware=hw(["cpu"], PY_OS, min_ram=4, macos_min="14.0"), tier="standard"))
recipes.append(recipe(
    id="pocket-tts-clone", display_name="Pocket TTS cloning (English)",
    engine={"kind": "python", "family": "pocket", "entry": "english_clone", "python": "3.12", "lock": "pocket"},
    artifacts=[pocket_nc, pocket_tok, pocket_clone], license=plic,
    gate={"provider": "huggingface", "repo": "kyutai/pocket-tts", "url": "https://huggingface.co/kyutai/pocket-tts",
          "form": "company + purpose; instant approval"},
    capabilities=caps(clone=True, clone_ref_seconds={"min": 3, "ideal": 10, "max": 30}, streaming="native", max_chunk_chars=500),
    languages=["en"], sample_rate=24000, hardware=hw(["cpu"], PY_OS, min_ram=4, macos_min="14.0"), tier="standard",
    notes_internal="Other Pocket languages (fr de pt it es) are v1.1."))

IPL = ["as", "bn", "brx", "doi", "en", "gu", "hi", "kn", "kok", "mai", "ml", "mni", "mr", "ne", "or", "sa", "sat", "sd", "ta", "te", "ur"]
recipes.append(recipe(
    id="indic-parler-tts", display_name="Indic Parler-TTS",
    engine={"kind": "python", "family": "parler", "entry": "indic", "python": "3.11", "lock": "parler"},
    artifacts=[art('indic-parler-tts'), {"source": "huggingface", "repo": "google/flan-t5-large", "revision": "0613663d0d48ea86ba8cb3d7a44f0f65dc596a2a", "gated": False,
               "files": [{"path": "config.json", "size": 662, "sha256": None}, {"path": "special_tokens_map.json", "size": 2201, "sha256": None},
                         {"path": "spiece.model", "size": 791656, "sha256": "d60acb128cf7b7f2536e8f38a5b18a05535c9e14c7a355904270e15b0945ea86"},
                         {"path": "tokenizer.json", "size": 2424064, "sha256": None}, {"path": "tokenizer_config.json", "size": 2539, "sha256": None}]}],
    license=lic("Apache-2.0", "https://huggingface.co/ai4bharat/indic-parler-tts", attribution="Indic Parler-TTS by AI4Bharat & Hugging Face (Apache-2.0)"),
    gate={"provider": "huggingface", "repo": "ai4bharat/indic-parler-tts", "url": "https://huggingface.co/ai4bharat/indic-parler-tts",
          "form": "shares your hugging face username and email; instant approval"},
    capabilities=caps(emotion="description", max_chunk_chars=250, delivery=DELIVERY),
    languages=IPL, sample_rate=44100,
    hardware=hw(["cuda", "mps", "cpu"], PY_OS, min_ram=8, min_vram=4, macos_min="14.0"), tier="standard",
    notes_internal="kok mai sat sd ur have no named speakers: description-only voices. CPU/MPS speed unmeasured."))
recipes.append(recipe(
    id="voxcpm2", display_name="VoxCPM2",
    engine={"kind": "python", "family": "voxcpm", "entry": "voxcpm2", "python": "3.12", "lock": "voxcpm"},
    artifacts=[art('voxcpm2')],
    license=lic("Apache-2.0", "https://huggingface.co/openbmb/VoxCPM2", attribution="VoxCPM2 by OpenBMB (Apache-2.0)",
                policy="Impersonation, fraud and disinformation are strictly forbidden by the model authors."),
    capabilities=caps(clone=True, clone_ref_seconds={"min": 3, "ideal": 10, "max": 30}, voice_design=True, streaming="native", max_chunk_chars=400),
    languages=["ar", "da", "de", "el", "en", "es", "fi", "fr", "he", "hi", "id", "it", "ja", "ko", "ms", "nl", "no", "pl", "pt", "ru", "sv", "sw", "th", "tr", "vi", "zh"],
    sample_rate=48000, hardware=hw(["cuda", "mps"], PY_OS, min_ram=16, min_vram=8, macos_min="14.0"), tier="advanced",
    notes_internal="Language list partial: confirm full 30 codes against the card before release (Hindi confirmed)."))

for r in recipes:
    with open(os.path.join(out_dir, r['id'] + '.json'), 'w', encoding='utf-8') as f:
        json.dump(r, f, indent=2, ensure_ascii=False)
        f.write("\n")
    print(r['id'], round(r['download_bytes'] / 1e6), 'MB')
