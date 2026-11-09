import json, sys, re
def L(o): return len(json.dumps(o,separators=(",",":"),ensure_ascii=False).encode())
for s in ["full","compact"]:
    d=json.load(open(s+".json"))
    print("=====",s)
    init=d["initialize"]
    if "result" not in init: print("init",str(init)[:300])
    else:
        r=init["result"]; print("protocol",r.get("protocolVersion"),"server",r.get("serverInfo"),"instructions bytes",len((r.get("instructions") or "").encode()), "init result bytes",L(r), "caps", list(r.get("capabilities",{}).keys()))
    t=d["tools/list"]
    if "result" not in t: print("tools",str(t)[:300]); continue
    tools=t["result"]["tools"]; print("tools",len(tools),"result bytes",L(t["result"]),"~tokens",L(t["result"])//4, "keys", list(t["result"].keys()))
    rows=[(x["name"],L(x),len((x.get("description") or "").encode()),L(x.get("inputSchema")), L({k:v for k,v in x.items() if k not in("name","description","inputSchema")})) for x in tools]
    rows.sort(key=lambda r:-r[1])
    print("sum desc",sum(r[2] for r in rows),"sum schema",sum(r[3] for r in rows),"sum other",sum(r[4] for r in rows))
    print("top12"); [print(r) for r in rows[:12]]
    print("bottom5"); [print(r) for r in rows[-5:]]
    tot=0
    for m in ["resources/list","resources/templates/list","prompts/list"]:
        x=d[m]; b=L(x["result"]) if "result" in x else 0; tot+=b; print(m,b, str(x.get("error",""))[:100])
    print("res+tmpl+prompts total",tot)
    # schema obs
    allj=json.dumps([x.get("inputSchema") for x in tools],separators=(",",":"))
    print("schema json bytes",len(allj))
    print("$defs count/bytes", allj.count('"$defs"'), sum(L(x["inputSchema"].get("$defs")) for x in tools if x.get("inputSchema",{}).get("$defs")))
    print("type-null unions", len(re.findall(r'"type":\[[^\]]*"null"[^\]]*\]',allj)), sum(len(m) for m in re.findall(r'"type":\[[^\]]*"null"[^\]]*\]',allj)))
    print("anyOf null", allj.count('"null"'))
    for pat in ['"format":"uint32"','"format":"uint"','"format":"int32"','"format":"uint64"','"format":"int64"','"minimum":0','"format":"double"','"format":"float"','"format":"uint8"','"format":"uint16"','"default":null','"additionalProperties":false','"$schema"','"title"']:
        c=allj.count(pat); print(pat,c,c*len(pat))
    # property descriptions
    descs=[]
    def walk(o,path=""):
        if isinstance(o,dict):
            for k,v in o.items():
                if k=="description" and isinstance(v,str) and path: descs.append((path,v))
                walk(v,path+"/"+k)
        elif isinstance(o,list):
            for i,v in enumerate(o): walk(v,path+f"[{i}]")
    for x in tools:
        walk(x.get("inputSchema"),x["name"])
    print("param desc count",len(descs),"bytes",sum(len(v.encode()) for _,v in descs))
    from collections import Counter
    c=Counter()
    for p,v in descs:
        name=re.findall(r'/properties/([^/]+)$',p)
        c[name[0] if name else p]+=len(v.encode())
    print("by param name top10",c.most_common(10))
    # repeated descriptions
    cc=Counter(v for _,v in descs)
    rep=[(v[:80],n,n*len(v.encode())) for v,n in cc.items() if n>1]
    rep.sort(key=lambda r:-r[2]); print("repeated desc (text,count,total bytes)",rep[:8], "total dup bytes", sum(n*len(v.encode())-len(v.encode()) for v,n in cc.items() if n>1))
    # project/projects
    pj=[(p,len(v.encode())) for p,v in descs if re.search(r'/properties/(project|projects|project_name)$',p)]
    print("project/projects params",len(pj),"desc bytes",sum(b for _,b in pj))
    long=sorted(descs,key=lambda pv:-len(pv[1]))[:5]
    print("longest param descs",[(p,len(v.encode())) for p,v in long])
    # property count and prop bytes of project
    pp=0;pc=0
    for x in tools:
        for k in ("project","projects"):
            pr=x.get("inputSchema",{}).get("properties",{}).get(k)
            if pr: pp+=L(pr)+len(k)+4;pc+=1
    print("project(s) property entries",pc,"bytes incl key",pp)
    print("pretty-printed tools/list bytes", len(json.dumps(t["result"],indent=2).encode()))
