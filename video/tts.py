#!/usr/bin/env python3
"""Narração ElevenLabs. Chave SOMENTE via env ELEVEN_API_KEY (nunca em arquivo/repo).
  python3 tts.py --list                      # lista vozes (e quais o plano permite)
  python3 tts.py VOICE_ID [--force s1,s2]    # gera audio/<id>.mp3 para cada cena de roteiro.json
roteiro.json: [{"id":"s1","txt":"..."}]"""
import json,os,sys,urllib.request
key=os.environ.get('ELEVEN_API_KEY') or sys.exit('defina ELEVEN_API_KEY')
if sys.argv[1]=='--list':
    r=urllib.request.Request('https://api.elevenlabs.io/v1/voices',headers={'xi-api-key':key})
    for v in json.load(urllib.request.urlopen(r))['voices']:
        l=v.get('labels',{});print(v['voice_id'],v['name'],l.get('gender'),l.get('language') or '',l.get('accent') or '',v.get('category'))
    sys.exit()
vid=sys.argv[1];force=sys.argv[sys.argv.index('--force')+1].split(',') if '--force' in sys.argv else []
os.makedirs('audio',exist_ok=True)
for s in json.load(open('roteiro.json')):
    f=f"audio/{s['id']}.mp3"
    if os.path.exists(f) and s['id'] not in force: continue
    body=json.dumps({"text":s['txt'],"model_id":"eleven_multilingual_v2","language_code":"pt","voice_settings":{"stability":0.5,"similarity_boost":0.8,"style":0.25,"use_speaker_boost":True}}).encode()
    r=urllib.request.Request(f"https://api.elevenlabs.io/v1/text-to-speech/{vid}?output_format=mp3_44100_128",body,{"xi-api-key":key,"Content-Type":"application/json"})
    try: open(f,'wb').write(urllib.request.urlopen(r,timeout=90).read());print('ok',s['id'])
    except Exception as e:
        print('ERRO',s['id'],getattr(e,'read',lambda:b'')()[:300] or e);sys.exit(1)
