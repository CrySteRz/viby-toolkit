"""Route every labelled probe through baseten/mammoth-1 as a `choice` question; record accuracy, latency, errors.

Usage: python3 -I mammoth_routing.py <toolkit_root> <out.jsonl> [reps]
Key from AI_GATEWAY_API_KEY.
"""
import json, os, re, sys, time, urllib.request, urllib.error, statistics, pathlib

root = pathlib.Path(sys.argv[1])
out = pathlib.Path(sys.argv[2])
reps = int(sys.argv[3]) if len(sys.argv) > 3 else 1
key = os.environ["AI_GATEWAY_API_KEY"]

skills = {}
for f in sorted((root / "plugins/viby-toolkit/skills").glob("*/SKILL.md")):
    text = f.read_text()
    m = re.search(r"^description:\s*(.+?)(?=^\w[\w-]*:|^---)", text, re.S | re.M)
    desc = re.sub(r"\s+", " ", m.group(1)).strip().strip("\"'>|").strip() if m else ""
    skills[f.parent.name] = desc[:600]
skills["none"] = "No skill fits: casual chat, a factual question, or a tiny one-line edit that needs no workflow."

probes = []
for tsv in sorted((root / "tests/routing").glob("probes*.tsv")):
    for line in tsv.read_text().splitlines():
        parts = line.split("\t")
        if len(parts) == 3:
            probes.append((tsv.name, parts[1].split(":")[-1], parts[2]))
# Exploratory negatives: no skill should fire.
for p in ["what's the capital of Australia", "thanks, that's all", "what does HTTP 418 mean",
          "rename the variable x to count on line 12", "what time is it in Tokyo", "explain what a mutex is in one sentence"]:
    probes.append(("negatives", "none", p))

def ask(prompt):
    body = json.dumps({"model": os.environ.get("MODEL", "baseten/mammoth-1"),
                       "state": f"A developer typed this request to their coding agent: {prompt!r}",
                       "questions": {"skill": {"type": "choice",
                                               "instructions": "Which workflow skill should handle this request? Pick none if no skill fits.",
                                               "criteria": skills}}}).encode()
    req = urllib.request.Request("https://ai-gateway.vercel.sh/v1/evaluate", body,
                                 {"Authorization": f"Bearer {key}", "Content-Type": "application/json"})
    t = time.monotonic()
    try:
        with urllib.request.urlopen(req, timeout=10) as r:
            data = json.load(r); code = r.status
    except urllib.error.HTTPError as e:
        data = {"error": e.read().decode()[:200]}; code = e.code
    except Exception as e:
        data = {"error": repr(e)[:200]}; code = 0
    return code, time.monotonic() - t, data

with out.open("w") as fh:
    for rep in range(reps):
        for src, want, prompt in probes:
            for attempt in range(4):
                code, dt, data = ask(prompt)
                if code != 429: break
                time.sleep(20)
            time.sleep(float(os.environ.get('PACE', '0')))
            a = data.get("answers", {}).get("skill", {})
            probs = a.get("probabilities", {})
            top = sorted(probs.items(), key=lambda kv: -kv[1])[:3]
            rec = {"rep": rep, "src": src, "want": want, "prompt": prompt, "code": code, "s": round(dt, 3),
                   "got": a.get("choice"), "conf": a.get("confidence"), "top3": top,
                   "tokens": data.get("usage", {}).get("inputTokens"), "err": data.get("error")}
            fh.write(json.dumps(rec) + "\n"); fh.flush()

rows = [json.loads(l) for l in out.read_text().splitlines()]
ok = [r for r in rows if r["code"] == 200]
print(f"skills offered: {len(skills)}  calls: {len(rows)}  http200: {len(ok)}  errors: {len(rows)-len(ok)}")
from collections import Counter
print("status codes:", dict(Counter(r["code"] for r in rows)))
lat = sorted(r["s"] for r in ok)
if lat:
    print(f"latency p50 {statistics.median(lat):.2f}s  p95 {lat[int(0.95*(len(lat)-1))]:.2f}s  max {lat[-1]:.2f}s  input tokens ~{ok[0]['tokens']}")
for grp in sorted({r["src"] for r in ok}):
    g = [r for r in ok if r["src"] == grp]
    hit = sum(r["got"] == r["want"] for r in g)
    print(f"{grp:28s} {hit}/{len(g)} = {hit/len(g):.0%}")
lab = [r for r in ok if r["src"] != "negatives"]
hit = sum(r["got"] == r["want"] for r in lab)
print(f"ALL labelled                 {hit}/{len(lab)} = {hit/len(lab):.0%}")
print("misses:")
for r in ok:
    if r["got"] != r["want"] and r["rep"] == 0:
        print(f"  [{r['src']}] want={r['want']} got={r['got']} conf={r['conf']:.2f} :: {r['prompt']}")
if reps > 1:
    by = {}
    for r in ok: by.setdefault((r["src"], r["prompt"]), set()).add(r["got"])
    print("unstable prompts across reps:", sum(len(v) > 1 for v in by.values()), "of", len(by))
