# Gera graph.json a partir do vault real (links [[...]]) com layout de força pré-simulado (keyframes).
import os,re,json,glob,numpy as np,sys
V=sys.argv[1]
files=[f for f in glob.glob(V+'/**/*.md',recursive=True) if '/.' not in f and '/Templates/' not in f]
name={os.path.splitext(os.path.basename(f))[0].lower():i for i,f in enumerate(files)}
E=set()
for i,f in enumerate(files):
    for m in re.findall(r'\[\[([^\]|#]+)',open(f,errors='ignore').read()):
        j=name.get(os.path.basename(m.strip()).lower())
        if j is not None and j!=i:E.add((min(i,j),max(i,j)))
E=list(E);n=len(files);deg=np.zeros(n)
for a,b in E:deg[a]+=1;deg[b]+=1
keep=np.where(deg>0)[0];idx={k:i for i,k in enumerate(keep)};n=len(keep)
E=np.array([(idx[a],idx[b]) for a,b in E]);deg=deg[keep]
lab=[os.path.splitext(os.path.basename(files[k]))[0] for k in keep]
def cat(p):
    p=os.path.relpath(p,V)
    if p.startswith('01'):return 0
    if p.startswith('02'):return 1
    if '/Conceitos/' in p or '/MOCs/' in p:return 2
    if 'Pesquisa' in p or 'Radar' in p or 'Mercado' in p:return 3
    if '/Claude/' in p or '/Gemini/' in p or '/GPT/' in p or '/Codex/' in p:return 5
    if 'Relatórios' in p:return 6
    return 4
fold=[cat(files[k]) for k in keep]
# ordem de aparição: BFS a partir do maior hub
from collections import deque
adj=[[] for _ in range(n)]
for a,b in E:adj[a].append(b);adj[b].append(a)
root=int(deg.argmax());order=[];seen={root};q=deque([root])
while q:
    u=q.popleft();order.append(u)
    for v in sorted(adj[u],key=lambda x:-deg[x]):
        if v not in seen:seen.add(v);q.append(v)
for u in np.argsort(-deg):
    if u not in seen:seen.add(u);order.append(int(u))
rank=np.zeros(n);rank[order]=np.arange(n)/n
rng=np.random.default_rng(3);P=rng.normal(0,1,(n,2))*400;Vel=np.zeros((n,2))
K=[];STEPS=300
for it in range(STEPS):
    d=P[:,None,:]-P[None,:,:];r2=(d**2).sum(-1)+25
    F=(d/(r2*np.sqrt(r2))[...,None]*90000).sum(1)
    s=P[E[:,1]]-P[E[:,0]];L=np.sqrt((s**2).sum(1))+1e-6
    w=(1/np.sqrt(np.minimum(deg[E[:,0]],deg[E[:,1]])))[:,None]
    f=s/L[:,None]*(L-25)[:,None]*0.12*w
    np.add.at(F,E[:,0],f);np.add.at(F,E[:,1],-f)
    F-=P*0.004
    Vel=(Vel+F)*0.82;step=np.clip(np.sqrt((Vel**2).sum(1)),0,30)
    Vel=Vel/ (np.sqrt((Vel**2).sum(1))+1e-9)[:,None]*step[:,None];P=P+Vel
    if it%10==0 or it==STEPS-1:K.append(P.copy())
c=K[-1].mean(0);sc=max(np.abs(K[-1]-c).max(0)/np.array([780,430]))
K=[((k-c)/sc).round(1).tolist() for k in K]
top=[int(i) for i in np.argsort(-deg)[:14]]
json.dump({"n":n,"e":E.tolist(),"deg":deg.astype(int).tolist(),"rank":rank.round(4).tolist(),"lab":lab,"fold":fold,"top":top,"K":K},open('graph.json','w'),ensure_ascii=False)
print(n,'nós',len(E),'arestas',len(K),'keyframes',[lab[i] for i in top[:8]])
