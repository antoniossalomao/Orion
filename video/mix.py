# Mix final: voz + trilha (abaixa sob a voz, sidechain) + efeitos (audio/sfx.wav, se existir); normaliza para ~-14 LUFS.
import json,subprocess,os
tl=json.load(open('timeline.json'));ins=[];fl=[];lab=[]
for i,s in enumerate(tl['scenes']):
    ins+=['-i',f"audio/{s['id']}.mp3"];ms=int(s['voice']*1000)
    fl.append(f"[{i}:a]adelay={ms}|{ms},volume=1.25[v{i}]");lab.append(f"[v{i}]")
m=len(tl['scenes']);ins+=['-i','audio/music.wav'];fx=os.path.exists('audio/sfx.wav')
if fx: ins+=['-i','audio/sfx.wav']
f=(";".join(fl)+f";{''.join(lab)}amix=inputs={m}:normalize=0,apad,aformat=sample_rates=44100:channel_layouts=stereo,"
   f"highpass=f=80,acompressor=threshold=0.1:ratio=3:attack=5:release=120,asplit=2[vo][vsc];"
   f"[{m}:a]volume=0.62[mu];[mu][vsc]sidechaincompress=threshold=0.03:ratio=5:attack=20:release=350[duck];"
   +(f"[{m+1}:a]volume=0.75[fx];[vo][duck][fx]amix=inputs=3" if fx else "[vo][duck]amix=inputs=2")
   +":normalize=0,loudnorm=I=-14:TP=-1.2:LRA=11,aresample=44100[out]")
subprocess.check_call(['ffmpeg','-y','-v','error']+ins+['-filter_complex',f,'-map','[out]','-t',str(tl['total']),'audio/mix.wav'])
print('audio/mix.wav')
