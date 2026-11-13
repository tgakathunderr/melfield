# Local API & CLI

## Local API
Off by default. Turn on in **Settings › Advanced › local api**. Binds to **127.0.0.1 only** (default port 5150). Optional bearer key (Settings › Advanced › require a key); the key is regenerated each time the API starts.

### `POST /v1/audio/speech` (OpenAI-compatible)
```json
{ "model": "tts-1", "input": "Hello from Melfield", "voice": "aria", "response_format": "wav", "speed": 1.0 }
```
- `voice`: a Melfield voice id or name (`aria`, `Kabir`, your clone's name). OpenAI names (`alloy`, `nova`…) map to your default voice.
- `response_format`: `wav` (default), `flac`, `pcm` (16-bit LE mono; rate in `x-sample-rate`). `mp3` returns 400 (not available yet).
- `speed`: 0.25–4.0 (clamped to 0.5–2.0 internally).
- `language` (Melfield extension): override language detection.
- Clone voices are always watermarked.

```python
from openai import OpenAI
client = OpenAI(base_url="http://127.0.0.1:5150/v1", api_key="unused-or-your-key")
client.audio.speech.create(model="tts-1", voice="aria", input="Hi!", response_format="wav").write_to_file("hi.wav")
```

### Other endpoints
- `GET /v1/voices` → `{data:[{id, name, languages, installed, kind}]}`
- `GET /v1/models` → installed recipes (OpenAI list shape)
- `GET /health` → `{ok, version}`

### Errors
OpenAI envelope: `{"error": {"message", "type", "code"}}` with Melfield's human message. 400 bad input · 401 key · 404 unknown voice · 409 voice not installed · 429 busy (2 concurrent, 16 queued).

## CLI
```
melfield list [--installed]
melfield pull <voice>
melfield say "text" [-v aria] [-o out.wav|.flac] [--speed 1.0] [--play]
melfield serve [--port 5150] [--token KEY]
```
Uses the same data folder as the app (`MELFIELD_HOME` overrides). `MELFIELD_DEBUG=1` prints error detail.
