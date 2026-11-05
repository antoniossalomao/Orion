#!/usr/bin/env python3
"""Gera timeline.json a partir de roteiro.json + duração real dos mp3.
Overrides opcionais em timeline_overrides.json: {"s1":[lead,tail]} (segundos antes/depois da fala)."""
import json,os,subprocess
r=json.load(open('roteiro.json'));ov=json.load(open('timeline_overrides.json')) if os.path.exists('timeline_overrides.json') else {}
t=0.0;out=[]
for s in r:
    d=float(subprocess.check_output(['ffprobe','-v','error','-show_entries','format=duration','-of','csv=p=0',f"audio/{s['id']}.mp3"]))
    lead,tail=ov.get(s['id'],(.55,.7))
    out.append({"id":s['id'],"txt":s['txt'],"start":round(t,3),"voice":round(t+lead,3),"dur":d,"end":round(t+lead+d+tail,3)});t=out[-1]['end']
json.dump({"total":round(t,3),"scenes":out},open('timeline.json','w'),ensure_ascii=False,indent=1)
for o in out:print(o['id'],o['start'],o['end'])
print('total',t)
