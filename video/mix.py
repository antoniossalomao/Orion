import json,subprocess
tl=json.load(open('timeline.json'));ins=[];fl=[];lab=[]
for i,s in enumerate(tl['scenes']):
    ins+=['-i',f"audio/{s['id']}.mp3"];ms=int(s['voice']*1000)
    fl.append(f"[{i}:a]adelay={ms}|{ms},volume=1.15[v{i}]");lab.append(f"[v{i}]")
ins+=['-i','audio/music.wav'];m=len(tl['scenes'])
f=";".join(fl)+f";{''.join(lab)}amix=inputs={m}:normalize=0,apad,aformat=sample_rates=44100:channel_layouts=stereo,asplit=2[vo][vsc];[{m}:a]volume=0.85[mu];[mu][vsc]sidechaincompress=threshold=0.02:ratio=6:attack=15:release=400[duck];[vo][duck]amix=inputs=2:normalize=0,alimiter=limit=0.95[out]"
subprocess.check_call(['ffmpeg','-y','-v','error']+ins+['-filter_complex',f,'-map','[out]','-t',str(tl['total']),'audio/mix.wav'])
