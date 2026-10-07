# PyInstaller: `uv run pyinstaller orion.spec --noconfirm` -> dist/orion/ (orion.exe no Windows).
# Pasta (onedir) em vez de arquivo único: abre mais rápido e antivírus implica menos com ela.
import sys
from pathlib import Path

from PyInstaller.utils.hooks import collect_all, collect_data_files, collect_submodules

raiz = Path(SPECPATH)
datas, binarios, ocultos = [], [], []

# pacotes que carregam módulos/dados por nome em tempo de execução
for pacote in ("mcp_types", "pydantic_settings", "keyring", "edge_tts", "google.genai"):
    d, b, h = collect_all(pacote)
    datas += d
    binarios += b
    ocultos += h
# `mcp.cli` importa typer e sai do processo se faltar: o Orion só usa o cliente
ocultos += collect_submodules("mcp", filter=lambda n: not n.startswith("mcp.cli"))
datas += collect_data_files("mcp")
ocultos += collect_submodules("uvicorn") + collect_submodules("orion")
ocultos += ["multipart", "multipart.multipart"]  # upload do front
datas += collect_data_files("docx") + collect_data_files("openpyxl")  # modelos de documento
if sys.platform == "win32":
    ocultos += ["keyring.backends.Windows"]

# a interface (front) é servida em /ui/: vai junto, sem o código Python que ele traz
front = raiz / "Orion_Core" / "Front_end_Orion"
for arq in front.rglob("*"):
    if arq.is_file() and arq.suffix not in (".py", ".pyc") and "__pycache__" not in arq.parts:
        datas.append((str(arq), str(Path("Orion_Core") / "Front_end_Orion" / arq.relative_to(front).parent)))
datas.append((str(raiz / "mcp.example.json"), "."))

a = Analysis(
    ["orion_exe.py"],
    pathex=[str(raiz)],
    binaries=binarios,
    datas=datas,
    hiddenimports=ocultos,
    excludes=["tkinter", "pytest", "playwright", "reportlab", "PIL", "pyright"],
    noarchive=False,
)
pyz = PYZ(a.pure)
exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name="orion",
    console=True,  # a janela mostra o log; fechá-la encerra o Orion
    icon=str(raiz / "assets" / "orion.ico"),
    upx=False,
)
coll = COLLECT(exe, a.binaries, a.datas, name="orion", upx=False)
