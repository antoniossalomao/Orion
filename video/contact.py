#!/usr/bin/env python3
"""python3 contact.py 5 20 40 ... -> build/sheet.jpg (grade 3 colunas) para conferir várias cenas de uma vez."""
import sys
from PIL import Image
ts=sys.argv[1:];im=Image.new('RGB',(1920,360*((len(ts)+2)//3)))
for i,t in enumerate(ts):im.paste(Image.open(f'build/p_{t}.jpg').resize((640,360)),((i%3)*640,(i//3)*360))
im.save('build/sheet.jpg');print('build/sheet.jpg')
