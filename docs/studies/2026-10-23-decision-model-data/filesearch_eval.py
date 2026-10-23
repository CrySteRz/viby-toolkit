"""Does a decision model reranking agentgrep `find`'s top files pick the right file more often?

Usage: MODEL=... PACE=12.5 python3 -I filesearch_eval.py <viby> <labels.jsonl> <out.jsonl>
Key from AI_GATEWAY_API_KEY. Per query: agentgrep find (10 files) -> rank of the answer; then a
`choice` over those files (criteria = each file's role + structure block) -> the model's pick.
"""
import json, os, re, subprocess, sys, time, urllib.request, urllib.error

viby, labels, out = sys.argv[1], sys.argv[2], sys.argv[3]
key, model, pace = os.environ["AI_GATEWAY_API_KEY"], os.environ["MODEL"], float(os.environ.get("PACE", "12.5"))

servers = {}
def find(root, query):
    if root not in servers:
        p = subprocess.Popen([viby, "agentgrep-server"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, cwd=root)
        p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "eval", "version": "0"}}}) + "\n"); p.stdin.flush()
        p.stdout.readline()
        p.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n"); p.stdin.flush()
        servers[root] = p
    p = servers[root]
    p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "agentgrep", "arguments": {"mode": "find", "query": query, "path": root, "max_files": int(os.environ.get("MAX_FILES", "10"))}}}) + "\n"); p.stdin.flush()
    text = json.loads(p.stdout.readline())["result"]["content"][0]["text"]
    blocks = re.split(r"\n(?=\d+\. )", text)
    files = []
    for b in blocks:
        m = re.match(r"(\d+)\. (\S+)\n(.*)", b, re.S)
        if m:
            files.append((m.group(2), re.sub(r"\s+", " ", m.group(3)).strip()[:500]))
    return files

def choose(query, files):
    body = {"model": model,
            "state": f"A developer asked their coding agent: {json.dumps(query)}. Candidate files from a code search are the options.",
            "questions": {"file": {"type": "choice",
                                   "instructions": "Which file should the developer open first to find what they asked about?",
                                   "criteria": {f: f"{f} — {d}" for f, d in files}}}}
    req = urllib.request.Request("https://ai-gateway.vercel.sh/v1/evaluate", json.dumps(body).encode(),
                                 {"Authorization": f"Bearer {key}", "Content-Type": "application/json"})
    for _ in range(4):
        t = time.monotonic()
        try:
            with urllib.request.urlopen(req, timeout=15) as r:
                a = json.load(r)["answers"]["file"]
                return a.get("choice"), a.get("confidence"), time.monotonic() - t
        except urllib.error.HTTPError as e:
            if e.code != 429:
                return f"HTTP {e.code}", None, 0
            time.sleep(20)
        except Exception as e:
            return f"ERR {e!r}"[:60], None, 0
    return "HTTP 429", None, 0

rows = [json.loads(l) for l in open(labels) if l.strip()]
with open(out, "w") as fh:
    for r in rows:
        files = find(r["root"], r["query"])
        paths = [f for f, _ in files]
        rank = paths.index(r["answer"]) + 1 if r["answer"] in paths else None
        pick, conf, dt = choose(r["query"], files) if files else (None, None, 0)
        rec = {**r, "ag_top": paths[:3], "ag_rank": rank, "pick": pick, "conf": conf, "s": round(dt, 2)}
        fh.write(json.dumps(rec) + "\n"); fh.flush()
        time.sleep(pace)

res = [json.loads(l) for l in open(out)]
n = len(res)
for repo in sorted({x["repo"] for x in res}) + ["ALL"]:
    g = [x for x in res if repo == "ALL" or x["repo"] == repo]
    inn = [x for x in g if x["ag_rank"]]
    print(f"{model} repo {repo}: n={len(g)}  answer in agentgrep top10 {len(inn)}  "
          f"agentgrep top1 {sum(x['ag_rank'] == 1 for x in g)}  top3 {sum((x['ag_rank'] or 99) <= 3 for x in g)}  "
          f"model pick correct {sum(x['pick'] == x['answer'] for x in g)}  "
          f"model fixed a non-top1 {sum(x['pick'] == x['answer'] and x['ag_rank'] != 1 for x in g)}  "
          f"model broke a top1 {sum(x['pick'] != x['answer'] and x['ag_rank'] == 1 for x in g)}")
errs = [x for x in res if isinstance(x["pick"], str) and x["pick"].startswith(("HTTP", "ERR"))]
print("errors:", len(errs))
