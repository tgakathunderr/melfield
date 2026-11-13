import json, sys, urllib.request, fnmatch
repos = {
 "qwen3-0.6b-base": ("Qwen/Qwen3-TTS-12Hz-0.6B-Base", None),
 "qwen3-1.7b-base": ("Qwen/Qwen3-TTS-12Hz-1.7B-Base", None),
 "qwen3-1.7b-customvoice": ("Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice", None),
 "chatterbox-ml-v3": ("ResembleAI/chatterbox", ["t3_mtl23ls_v3.safetensors","s3gen.pt","ve.pt","grapheme_mtl_merged_expanded_v1.json","conds.pt","Cangjie5_TC.json"]),
 "chatterbox-turbo": ("ResembleAI/chatterbox-turbo", None),
 "pocket-tts": ("kyutai/pocket-tts", None),
 "pocket-tts-nc": ("kyutai/pocket-tts-without-voice-cloning", None),
 "indic-parler-tts": ("ai4bharat/indic-parler-tts", None),
 "voxcpm2": ("openbmb/VoxCPM2", None),
}
out = {}
for k,(repo,allow) in repos.items():
    try:
        info = json.load(urllib.request.urlopen(f"https://huggingface.co/api/models/{repo}"))
        sha = info["sha"]
        tree = json.load(urllib.request.urlopen(f"https://huggingface.co/api/models/{repo}/tree/{sha}?recursive=true"))
        files = [t for t in tree if t["type"]=="file"]
        if allow: files=[f for f in files if f["path"] in allow]
        out[k] = {"repo":repo,"revision":sha,"gated":info.get("gated",False),
                  "files":[{"path":f["path"],"size":f["size"],"sha256":(f.get("lfs") or {}).get("oid")} for f in files]}
    except Exception as e:
        out[k] = {"repo":repo,"error":str(e)}
json.dump(out, open(sys.argv[1],"w"), indent=1)
for k,v in out.items():
    if "error" in v: print(k, "ERR", v["error"]); continue
    print(k, v["revision"][:10], v["gated"], len(v["files"]), round(sum(f["size"] for f in v["files"])/1e9,3), "GB")
