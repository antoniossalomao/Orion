#!/usr/bin/env python3
"""timeline.json a partir de roteiro.json + duração real dos mp3, com o fim de cada cena
quantizado na batida (120 BPM = 0,5 s) para os cortes caírem no tempo da trilha.
Overrides em timeline_overrides.json: {"s1":[lead,tail]}."""
import json,os,subprocess,math
BEAT=0.5
r=json.load(open('roteiro.json'));ov=json.load(open('timeline_overrides.json')) if os.path.exists('timeline_overrides.json') else {}
t=0.0;out=[]
for s in r:
    d=float(subprocess.check_output(['ffprobe','-v','error','-show_entries','format=duration','-of','csv=p=0',f"audio/{s['id']}.mp3"]))
    lead,tail=ov.get(s['id'],(.55,.7))
    end=math.ceil((t+lead+d+tail)/BEAT)*BEAT
    out.append({"id":s['id'],"txt":s['txt'],"start":round(t,3),"voice":round(t+lead,3),"dur":d,"end":round(end,3)});t=end
json.dump({"total":round(t,3),"beat":BEAT,"scenes":out},open('timeline.json','w'),ensure_ascii=False,indent=1)
for o in out:print(o['id'],o['start'],o['end'])
print('total',t)
