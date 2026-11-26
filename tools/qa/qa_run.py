"""Executa áreas de QA e imprime o JSON de resultado (a última linha de stdout é o caminho do JSON)."""
import argparse
import importlib
import json
import sys
from datetime import date
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import perfil  # noqa: E402
from areas import ORDEM  # noqa: E402
from qa_lib import RAIZ, Sessao, json_out  # noqa: E402


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--area", default="todas", help="todas ou lista separada por vírgula: " + ",".join(ORDEM))
    ap.add_argument("--modo", choices=["relatorio", "corrigir"], default="relatorio")
    ap.add_argument("--docx", help="caminho do .docx com chaves de API (lido só em memória)")
    ap.add_argument("--setup", help="Orion Setup.exe (área instalador)")
    ap.add_argument("--bin", help="pasta com orion.exe e orion-app.exe (padrão: target/release|debug ou cargo run)")
    ap.add_argument("--saida", help="arquivo do JSON (padrão: qa-prints/<data>/resultado.json)")
    ap.add_argument("--limpar", action="store_true", help="apaga o perfil de teste ao final")
    a = ap.parse_args()

    if sys.platform != "win32":
        print(json.dumps({"erro": "QA de usuário só roda no Windows 11 com desktop"}), file=sys.stderr)
        return 2
    areas = ORDEM if a.area == "todas" else [x.strip() for x in a.area.split(",")]
    desconhecidas = [x for x in areas if x not in ORDEM]
    if desconhecidas:
        print(f"área desconhecida: {desconhecidas}", file=sys.stderr)
        return 2

    chaves = perfil.chaves_docx(a.docx)
    ctx = {"chaves": chaves, "orion": perfil.exe("orion", a.bin), "app": perfil.exe("orion-app", a.bin), "setup": a.setup}
    ctx["env"] = perfil.ambiente(chaves, "instalador")
    hoje = RAIZ / "qa-prints" / date.today().isoformat()
    resultados = []
    for nome in areas:
        s = Sessao(nome, hoje / nome)
        try:
            importlib.import_module(f"areas.{nome}").executar(s, ctx)
        except Exception as e:  # noqa: BLE001
            with s.passo(f"erro fatal da área {nome}"):
                raise e
        resultados.append(s.resultado())

    resumo = {
        "data": date.today().isoformat(),
        "modo": a.modo,
        "chaves_carregadas": sorted(chaves),  # só os NOMES das variáveis, nunca os valores
        "areas": resultados,
        "total_falhas": sum(r["falhou"] for r in resultados),
        "prints_das_falhas": [f["print"] for r in resultados for f in r["falhas"]],
        "proximo_passo": "abrir só os prints das falhas e corrigir (branch qa/<data>, PR)" if a.modo == "corrigir" else "gerar relatório com os prints das falhas",
    }
    saida = Path(a.saida) if a.saida else hoje / "resultado.json"
    saida.parent.mkdir(parents=True, exist_ok=True)
    saida.write_text(json_out(resumo), encoding="utf-8")
    if a.limpar:
        perfil.limpar()
    print(saida)
    return 1 if resumo["total_falhas"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
