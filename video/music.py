import numpy as np, json, wave
sr=44100; tl=json.load(open('timeline.json')); T=tl['total']; n=int(T*sr); t=np.arange(n)/sr
S={s['id']:s for s in tl['scenes']}; import sys
rev=S[sys.argv[1] if len(sys.argv)>1 else 's3']['start']  # cena da revelação do logo: riser antes, impacto + groove depois
rng=np.random.default_rng(7)
def lp(x,k):  # simple moving-average lowpass
    c=np.cumsum(np.insert(x,0,0)); y=(c[k:]-c[:-k])/k; return np.concatenate([np.zeros(k-1),y])
L=np.zeros(n);R=np.zeros(n)
# pad chords
ch=[[110,130.81,164.81],[87.31,130.81,174.61],[130.81,164.81,196],[98,146.83,196]]
pad=np.zeros(n)
for i,f0 in enumerate([f for c in ch for f in [c]]):
    pass
seg=8.0
for k in range(int(T/seg)+1):
    c=ch[k%4]; a=int(k*seg*sr); b=min(n,int((k+1)*seg*sr+sr*1.5))
    if a>=n: break
    tt=t[a:b]-k*seg; env=np.minimum(1,tt/2.5)*np.minimum(1,(seg+1.5-tt)/1.5)
    for f in c:
        for dt in (-0.4,0,0.4):
            ff=f*2**(dt/12*0.1)
            w=sum(np.sin(2*np.pi*ff*h*tt+h)/h for h in (1,2,3,4))
            pad[a:b]+=env*w*0.05
pad=lp(pad,12)
gain=np.interp(t,[0,rev-3,rev,T-4,T],[0.35,0.5,1.0,0.9,0.0]); pad*=gain
# heartbeat before reveal
hb=np.zeros(n)
for tb in np.arange(1.0,rev,1.1):
    i=int(tb*sr); m=int(0.35*sr); tt=np.arange(m)/sr
    hb[i:i+m]+=np.sin(2*np.pi*(55-30*tt)*tt)*np.exp(-tt*9)*0.5*np.interp(tb,[0,rev],[0.4,1])
# riser
ri0=rev-4.5; i0=int(ri0*sr); i1=int(rev*sr); m=i1-i0
nz=rng.standard_normal(m); nz=nz-lp(nz,40)  # highpass-ish
rise=nz*(np.linspace(0,1,m)**3)*0.28
sw=np.sin(2*np.pi*np.cumsum(np.linspace(120,1800,m))/sr)*np.linspace(0,1,m)**2*0.06
ri=np.zeros(n); ri[i0:i1]=rise+sw
# impact
im=np.zeros(n); i=i1; m=int(3.5*sr); tt=np.arange(m)/sr
im[i:i+m]=(np.sin(2*np.pi*(48-20*np.minimum(tt,1))*tt)*np.exp(-tt*1.4)*0.9+lp(rng.standard_normal(m),6)*np.exp(-tt*3)*0.35)
# groove from reveal: kick + hat + bass
gr=np.zeros(n); bpm=104; beat=60/bpm; tb=rev+beat
bass=np.zeros(n)
while tb<T-3:
    i=int(tb*sr); m=int(0.3*sr); tt=np.arange(m)/sr
    if i+m>=n: break
    gr[i:i+m]+=np.sin(2*np.pi*(120*np.exp(-tt*28)+45)*tt)*np.exp(-tt*10)*0.55
    tb+=beat
th=rev+beat/2; 
while th<T-3:
    i=int(th*sr); m=int(0.05*sr)
    if i+m>=n: break
    gr[i:i+m]+=(rng.standard_normal(m)-lp(rng.standard_normal(m),5))*np.exp(-np.arange(m)/sr*80)*0.08*(1 if int((th-rev)/beat)%2 else .6)
    th+=beat
roots=[55,43.65,65.41,49]
for k in range(int(T/seg)+1):
    r=roots[k%4]; a=int(max(k*seg,rev)*sr); b=min(n,int((k+1)*seg*sr))
    if a>=b or (k+1)*seg<rev: continue
    tt=t[a:b]; bass[a:b]=np.sin(2*np.pi*r*tt)*0.2*np.interp(tt,[rev,rev+4],[0,1])*(0.6+0.4*np.sin(2*np.pi*tt/beat*2)**2)
fade=np.interp(t,[0,T-3.5,T],[1,1,0])
mono=(pad+hb+ri+im+gr+bass)*fade
# stereo width
d=int(0.012*sr); L=mono+np.roll(pad,d)*0.3; R=mono+np.roll(pad,-d)*0.3
st=np.stack([L,R],1); st/=max(1e-9,np.abs(st).max())*1.15
w=wave.open('audio/music.wav','wb'); w.setnchannels(2); w.setsampwidth(2); w.setframerate(sr); w.writeframes((st*32767).astype('<i2').tobytes()); w.close()
