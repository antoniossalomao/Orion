# WebP animado para o README a partir do MP4 (Pillow, quadros inteiros: sem artefatos de diferença entre quadros).
import subprocess, sys
from PIL import Image
W, H, FPS = 1280, 720, 15
src, out = sys.argv[1], sys.argv[2]
p = subprocess.Popen(['ffmpeg', '-v', 'error', '-i', src, '-vf', f'fps={FPS},scale={W}:{H}:flags=lanczos', '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'], stdout=subprocess.PIPE)
def frames():
    while True:
        b = p.stdout.read(W * H * 3)
        if len(b) < W * H * 3: return
        yield Image.frombytes('RGB', (W, H), b)
it = frames(); first = next(it)
first.save(out, save_all=True, append_images=it, duration=int(1000 / FPS), loop=0, quality=int(sys.argv[3]) if len(sys.argv) > 3 else 70, method=4, minimize_size=False, allow_mixed=False)
print(out)
