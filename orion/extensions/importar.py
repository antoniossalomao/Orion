"""Importar plugin no formato do Claude Code ou do Codex (E8.3, regra 57).

Só viaja o que tem equivalente no Orion: as **skills** (texto). Servidores MCP, hooks, comandos e
agentes ficam de fora e entram numa lista "ignorado" que a pessoa lê **antes** de instalar. O
resultado é um pacote Orion comum (`manifest.json` + skills) que segue o caminho de sempre: instala
inativo, sem concessão, e só vale depois da revisão e da ativação (regra 45).

Formato do Claude Code: `.claude-plugin/plugin.json` + `skills/<nome>/SKILL.md` (+ `.mcp.json`,
`commands/`, `agents/`, `hooks/`). O do Codex segue a mesma ideia em `.codex-plugin/plugin.json`
(suporte por semelhança, nunca testado contra um plugin real do Codex).
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field

import yaml

from .installer import MAX_FILE, Package, fingerprint
from .plugins import PluginError, parse_manifest
from .skills import SkillHeader

FORMATOS = {
    ".claude-plugin/plugin.json": "claude-code",
    ".codex-plugin/plugin.json": "codex",
}
_PERMITIDOS = {"name", "description", "license", "compatibility", "metadata", "allowed-tools"}
_FRONT = re.compile(r"\A---\s*\n(.*?)\n---\s*(?:\n|\Z)", re.DOTALL)
_SEMVER = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+(?:-[a-z0-9.-]+)?$")
_ID = re.compile(r"[^a-z0-9_-]+")
# pastas conhecidas dos dois formatos que o Orion não tem como executar
_SEM_EQUIVALENTE = {
    "commands": "comandos de barra",
    "agents": "agentes",
    "hooks": "hooks (rodariam código na sua máquina)",
    "output-styles": "estilos de saída",
}


@dataclass(frozen=True)
class Conversao:
    package: Package
    formato: str
    ignorado: list[str] = field(default_factory=list)

    def resumo(self) -> dict:
        m = self.package.manifest
        return {
            "formato": self.formato,
            "id": m.id,
            "name": m.name,
            "version": m.version,
            "description": m.description,
            "license": m.license,
            "skills": [s.rsplit("/", 1)[-1] for s in m.skills],
            "ignorado": self.ignorado,
            "digest": self.package.digest,
        }


def _sem_pasta_envolvente(files: dict[str, bytes]) -> dict[str, bytes]:
    """Zip de uma pasta (`meu-plugin/...`): tira o prefixo comum se o marcador só aparece dentro."""
    if any(p in files for p in (*FORMATOS, "manifest.json")):
        return files
    topos = {p.split("/", 1)[0] for p in files}
    if len(topos) != 1 or any("/" not in p for p in files):
        return files
    prefixo = next(iter(topos)) + "/"
    return {p[len(prefixo) :]: d for p, d in files.items()}


def detectar(files: dict[str, bytes]) -> str:
    if "manifest.json" in files:
        return "orion"
    for caminho, formato in FORMATOS.items():
        if caminho in files:
            return formato
    raise PluginError("import_format_unknown")


def _id(nome: str) -> str:
    id_ = _ID.sub("-", nome.strip().lower()).strip("-_")[:48]
    if not re.fullmatch(r"[a-z0-9][a-z0-9_-]{0,47}", id_):
        raise PluginError("import_name_invalid")
    return id_


def _skill(pasta: str, dado: bytes, ignorado: list[str]) -> bytes | None:
    try:
        texto = dado.decode("utf-8")
    except UnicodeError:
        ignorado.append(f"skill {pasta}: SKILL.md não é UTF-8")
        return None
    achou = _FRONT.match(texto)
    if not achou:
        ignorado.append(f"skill {pasta}: sem frontmatter")
        return None
    try:
        cab = yaml.safe_load(achou.group(1))
    except yaml.YAMLError:
        ignorado.append(f"skill {pasta}: frontmatter ilegível")
        return None
    if not isinstance(cab, dict):
        ignorado.append(f"skill {pasta}: frontmatter ilegível")
        return None
    fora = sorted(str(k) for k in cab if k not in _PERMITIDOS or k == "allowed-tools")
    novo = {k: v for k, v in cab.items() if k in _PERMITIDOS and k != "allowed-tools"}
    novo["name"] = str(novo.get("name") or pasta)
    if isinstance(novo.get("metadata"), dict):
        novo["metadata"] = {str(k): str(v) for k, v in novo["metadata"].items()}
    elif "metadata" in novo:
        del novo["metadata"]
    for k in ("description", "license", "compatibility"):
        if k in novo:
            novo[k] = str(novo[k])
    try:
        SkillHeader.model_validate(novo)
    except ValueError:
        ignorado.append(f"skill {pasta}: nome ou descrição fora do formato do Orion")
        return None
    if novo["name"] != pasta:
        ignorado.append(f"skill {pasta}: o nome ({novo['name']}) difere da pasta")
        return None
    if fora:
        ignorado.append(f"skill {pasta}: campos ignorados ({', '.join(fora)})")
    corpo = texto[achou.end() :]
    cabecalho = yaml.safe_dump(novo, allow_unicode=True, sort_keys=False).strip()
    return f"---\n{cabecalho}\n---\n{corpo}".encode()


def converter(files: dict[str, bytes]) -> Conversao:
    """Converte os arquivos de um ZIP no pacote Orion. Levanta `PluginError` com um código."""
    files = _sem_pasta_envolvente(files)
    formato = detectar(files)
    if formato == "orion":
        pacote = Package(parse_manifest(files["manifest.json"]), fingerprint(files), files)
        return Conversao(pacote, "orion")
    try:
        bruto = json.loads(files[next(c for c, f in FORMATOS.items() if f == formato)])
    except (ValueError, UnicodeError):
        raise PluginError("import_manifest_invalid") from None
    if not isinstance(bruto, dict) or not isinstance(bruto.get("name"), str):
        raise PluginError("import_manifest_invalid")
    ignorado: list[str] = []
    versao = str(bruto.get("version") or "")
    if not _SEMVER.match(versao):
        ignorado.append(f"versão {'inválida' if versao else 'ausente'}: usei 0.1.0")
        versao = "0.1.0"
    novos: dict[str, bytes] = {}
    skills: list[str] = []
    pastas = sorted(
        {p.split("/")[1] for p in files if p.startswith("skills/") and p.count("/") >= 2}
    )
    for pasta in pastas:
        principal = files.get(f"skills/{pasta}/SKILL.md")
        if principal is None:
            ignorado.append(f"skill {pasta}: sem SKILL.md")
            continue
        convertida = _skill(pasta, principal, ignorado)
        if convertida is None:
            continue
        novos[f"skills/{pasta}/SKILL.md"] = convertida
        skills.append(f"skills/{pasta}")
        novos.update(  # referências e anexos vão como estão (scripts não rodam)
            {
                c: d
                for c, d in files.items()
                if c.startswith(f"skills/{pasta}/") and c != f"skills/{pasta}/SKILL.md"
            }
        )
    for pasta, o_que in _SEM_EQUIVALENTE.items():
        n = sum(1 for p in files if p.startswith(pasta + "/"))
        if n:
            ignorado.append(f"{pasta}/: {n} arquivo(s) de {o_que}, sem equivalente no Orion")
    if ".mcp.json" in files or "mcpServers" in bruto or "mcp" in bruto:
        try:
            servidores = sorted(json.loads(files[".mcp.json"]).get("mcpServers") or {})
        except (KeyError, ValueError, AttributeError):
            servidores = []
        quais = f" ({', '.join(servidores)})" if servidores else ""
        ignorado.append(
            f"servidores MCP{quais}: não viajam em pacote; cadastre-os em Conexões MCP, onde "
            "você revisa o comando e a classe de risco"
        )
    if not skills:
        raise PluginError("import_nothing_to_import")
    descricao = str(bruto.get("description") or "").strip() or f"Importado: {bruto['name']}"
    manifesto = {
        "schema_version": 1,
        "orion_api": 1,
        "id": _id(bruto["name"]),
        "version": versao,
        "name": str(bruto.get("displayName") or bruto["name"])[:80],
        "description": descricao[:1024],
        "license": str(bruto.get("license") or "LicenseRef-Importado")[:128],
        "skills": skills,
        "capabilities": [],
    }
    dados = json.dumps(manifesto, ensure_ascii=False, indent=1).encode()
    novos["manifest.json"] = dados
    if any(len(d) > MAX_FILE for d in novos.values()):
        raise PluginError("bundle_size_limit")
    pacote = Package(parse_manifest(dados), fingerprint(novos), novos)
    return Conversao(pacote, formato, ignorado)
