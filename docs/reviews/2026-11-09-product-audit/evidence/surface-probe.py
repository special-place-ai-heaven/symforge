import json, os, subprocess, sys, threading, queue, time
EXE = sys.argv[1]; surface = sys.argv[2]; cwd = sys.argv[3]; out = sys.argv[4]
env = dict(os.environ); env["SYMFORGE_NO_DAEMON"]="1"; env["SYMFORGE_AUTO_INDEX"]="false"
if surface == "compact": env["SYMFORGE_SURFACE"]="compact"
else: env.pop("SYMFORGE_SURFACE", None)
p = subprocess.Popen([EXE], cwd=cwd, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, creationflags=0x08000000)
q = queue.Queue()
def rd():
    for line in p.stdout: q.put(line)
    q.put(None)
threading.Thread(target=rd, daemon=True).start()
def send(o): p.stdin.write((json.dumps(o)+"\n").encode()); p.stdin.flush()
def rpc(i, m, params=None, to=60):
    send({"jsonrpc":"2.0","id":i,"method":m,"params":params or {}})
    end=time.time()+to
    while time.time()<end:
        try: l=q.get(timeout=1)
        except queue.Empty: continue
        if l is None: return {"error":"eof"}
        try: r=json.loads(l)
        except Exception: continue
        if r.get("id")==i: return r
    return {"error":"timeout"}
res={}
try:
    r=rpc(1,"initialize",{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"probe","version":"0"}})
    res["initialize"]=r
    send({"jsonrpc":"2.0","method":"notifications/initialized"})
    for i,m in enumerate(["tools/list","resources/list","resources/templates/list","prompts/list"],2):
        res[m]=rpc(i,m)
finally:
    p.kill()
    try: err=p.stderr.read(2000).decode(errors="replace")
    except Exception: err=""
    res["stderr_head"]=err
json.dump(res, open(out,"w"))
print("done")
