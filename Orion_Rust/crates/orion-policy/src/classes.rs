//! Classes de risco por ferramenta (REGRAS.md: classe de risco fixa por ferramenta).
//!
//! A classe é fixa por ferramenta e declarada aqui, não inferida por regex no texto da chamada.
//! O argumento dá só o refinamento (comando de shell, caminho).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    /// Roda direto.
    Read,
    /// Roda e fica no audit.
    Write,
    /// Shell, UI, agente: confirma, salvo leitura provada.
    Exec,
    /// Sempre confirma.
    Destructive,
}

impl Risk {
    pub fn as_str(self) -> &'static str {
        match self {
            Risk::Read => "read",
            Risk::Write => "write",
            Risk::Exec => "exec",
            Risk::Destructive => "destructive",
        }
    }

    pub fn parse(s: &str) -> Option<Risk> {
        match s {
            "read" => Some(Risk::Read),
            "write" => Some(Risk::Write),
            "exec" => Some(Risk::Exec),
            "destructive" => Some(Risk::Destructive),
            _ => None,
        }
    }
}

impl std::fmt::Display for Risk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToolSpec {
    pub name: String,
    pub risk: Option<Risk>,
    /// Argumento com comando de shell (EXEC).
    pub cmd_arg: Option<String>,
    /// Argumento com caminho que a ferramenta escreve.
    pub path_arg: Option<String>,
    /// `path_arg` é uma pasta que será reorganizada.
    pub organize: bool,
    /// Devolve conteúdo não confiável (web, e-mail, arquivos).
    pub external: bool,
    /// Argumento com caminho que a ferramenta LÊ (segredo confirma).
    pub read_path_arg: Option<String>,
    /// O modelo escolhe PARA ONDE a ferramenta fala na rede (URL, host): canal de exfiltração
    /// por GET. Depois de ler conteúdo externo, a sessão confirma antes de cada uso
    /// (REGRAS.md: conteúdo externo é dado, não instrução).
    pub egress: bool,
    pub origin: Option<String>,
    pub revision: Option<String>,
    pub require_confirmation: bool,
    pub masked_args: Vec<String>,
    pub display_name: Option<String>,
    pub origin_label: Option<String>,
    pub scope: Option<String>,
}

impl ToolSpec {
    pub fn new(name: impl Into<String>, risk: Risk) -> Self {
        ToolSpec { name: name.into(), risk: Some(risk), ..Default::default() }
    }

    /// Classe efetiva; ferramenta sem classe nunca chega aqui (o registro nega antes).
    pub fn risk(&self) -> Risk {
        self.risk.unwrap_or(Risk::Exec)
    }

    pub fn cmd(mut self, a: &str) -> Self {
        self.cmd_arg = Some(a.into());
        self
    }
    pub fn path(mut self, a: &str) -> Self {
        self.path_arg = Some(a.into());
        self
    }
    pub fn read_path(mut self, a: &str) -> Self {
        self.read_path_arg = Some(a.into());
        self
    }
    pub fn organize(mut self) -> Self {
        self.organize = true;
        self
    }
    pub fn external(mut self) -> Self {
        self.external = true;
        self
    }
    pub fn egress(mut self) -> Self {
        self.egress = true;
        self
    }
    pub fn confirm(mut self) -> Self {
        self.require_confirmation = true;
        self
    }
}

/// As 55 ferramentas do legado, nos nomes em PT (contrato de function-calling), mais as da
/// reescrita.
pub fn default_tools() -> HashMap<String, ToolSpec> {
    use Risk::*;
    let t = ToolSpec::new;
    let lista = vec![
        // leitura
        t("ler_arquivo", Read).read_path("path"),
        t("listar_arquivos", Read).read_path("path"),
        t("ler_clipboard", Read).external(),
        t("analisar_clipboard_com_ia", Read).external(),
        t("ler_documento", Read).external().read_path("path"),
        t("resumir_documento", Read).external(),
        t("transcrever_audio", Read).external().read_path("path"),
        t("traduzir_texto", Read),
        t("consultar_git", Read).read_path("repo_path"),
        t("buscar_memoria", Read),
        t("listar_numeros", Read),
        t("status_processo_bg", Read),
        t("listar_processos_bg", Read),
        t("listar_vigilancias", Read),
        t("consultar_audit_log", Read),
        t("checar_saude_sistema", Read),
        t("checar_servicos_orion", Read),
        t("analisar_imagem", Read).external().read_path("path"),
        t("ler_texto_imagem", Read).external().read_path("path"),
        // rótulos com hora, sem imagem (REGRAS.md: a câmera só liga por pedido seu, e cada uso
        // fica registrado)
        t("onde_vi", Read),
        t("consultar_clima", Read),
        t("obter_topico_celular", Read),
        t("status_enxame", Read),
        t("listar_eventos", Read).external(),
        // leitura de conteúdo externo (fonte de prompt injection)
        t("pesquisar_internet", Read).external(),
        t("pesquisar_com_ia", Read).external(),
        t("buscar_url", Read).external().egress(),
        t("ler_emails", Read).external(),
        t("ler_email", Read).external(),
        // escrita (com log)
        t("escrever_arquivo", Write).path("path"),
        t("gerar_documento", Write).path("path"),
        t("organizar_pasta", Write).path("path").organize(),
        t("escrever_clipboard", Write),
        t("salvar_memoria", Write),
        t("registrar_numero", Write),
        t("gerenciar_lembretes", Write),
        t("gerenciar_agendamentos", Write),
        t("gerenciar_tarefas", Write),
        t("notificar_usuario", Write),
        t("notificar_celular", Write),
        t("criar_rascunho_email", Write),
        t("criar_evento", Write),
        t("gerar_imagem", Write),
        t("capturar_tela", Write), // deixa um arquivo no computador; não sai dele
        t("abrir_app", Exec),      // abre programa ou arquivo qualquer: confirma
        t("controlar_midia", Write),
        t("backup_memoria", Write),
        t("iniciar_vigilancia_pasta", Write),
        t("parar_vigilancia_pasta", Write),
        t("criar_enxame", Write),
        t("consolidar_enxame", Write),
        // execução
        t("executar_comando", Exec).cmd("cmd"),
        t("iniciar_processo_bg", Exec).cmd("comando"),
        t("controlar_janela", Exec).external(), // título de aba é texto de terceiros
        t("explicar_tela", Exec).external(),    // envia a tela inteira ao modelo: confirma
        // foto da webcam: confirma sempre (REGRAS.md: a câmera só liga por pedido seu)
        t("ver_camera", Exec).external(),
        t("preparar_imagem_flow", Exec), // abre o navegador; não gera nem baixa (E11.8)
        t("navegar_web", Exec).external().egress(),
        t("consultar_especialista", Exec),
        // reescrita (fase 2)
        t("listar_fatos", Read),
        t("esquecer_fato", Destructive),
        t("editar_fato", Write).confirm(),
        t("delegar", Exec),
    ];
    lista.into_iter().map(|s| (s.name.clone(), s)).collect()
}

/// Limites do legado (chamadas, janela em segundos), mantidos.
pub fn default_rate_limits() -> HashMap<String, (u32, u64)> {
    [
        ("executar_comando", (20, 300)),
        ("iniciar_processo_bg", (5, 300)),
        ("escrever_arquivo", (30, 60)),
        ("organizar_pasta", (3, 300)),
        ("consultar_especialista", (3, 600)),
        ("navegar_web", (10, 300)),
        ("gerar_imagem", (5, 600)), // protege a cota gratuita do provedor
        ("pesquisar_internet", (20, 300)),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}
