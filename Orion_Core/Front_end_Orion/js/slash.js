/* ==========================================================================
   ORION — slash.js | comandos de barra do campo de mensagem (puro, sem DOM)
   "/" no começo do texto abre a lista; o que casa com um comando não vai para o modelo.
   ========================================================================== */
(function (raiz, fabrica) {
    const api = fabrica(typeof module === 'object' ? require('./util.js') : raiz.Orion.util);
    if (typeof module === 'object' && module.exports) module.exports = api;
    else (raiz.Orion = raiz.Orion || {}).slash = api;
})(typeof window !== 'undefined' ? window : globalThis, function (U) {
    'use strict';

    /** `args`: valores fixos aceitos; `livre`: aceita texto qualquer (ex.: /buscar) */
    const COMANDOS = [
        { nome: 'nova', desc: 'Nova conversa' },
        { nome: 'buscar', livre: 'texto', desc: 'Buscar na conversa' },
        { nome: 'copiar', args: ['resposta', 'conversa'], desc: 'Copiar a última resposta ou a conversa' },
        { nome: 'exportar', desc: 'Exportar a conversa em Markdown' },
        { nome: 'limpar', desc: 'Limpar o histórico da sessão' },
        { nome: 'projeto', livre: 'nome', desc: 'Mover a conversa para um projeto ("nenhum" tira do projeto)' },
        { nome: 'modelo', args: ['auto', 'groq', 'gemini', 'claude'], desc: 'Trocar o modelo' },
        { nome: 'tema', args: ['noite', 'grafite', 'contraste'], desc: 'Trocar o tema' },
        { nome: 'foco', desc: 'Modo foco (sem barras)' },
        { nome: 'mudo', desc: 'Ligar ou desligar a resposta por voz' },
        { nome: 'voz', desc: 'Voz ao vivo' },
        { nome: 'inicio', desc: 'Ir para o início' },
        { nome: 'memoria', desc: 'Ir para a memória' },
        { nome: 'integracoes', desc: 'Ir para as integrações' },
        { nome: 'config', desc: 'Ir para as configurações' },
        { nome: 'painel', desc: 'Ir para o painel (modelos, CLIs, aprovações, política)' },
        { nome: 'ajuda', desc: 'Ver os atalhos' },
    ];
    const POR_NOME = new Map(COMANDOS.map(c => [c.nome, c]));

    const dividir = texto => {
        const m = /^\/(\S*)(?:\s+([\s\S]*))?$/.exec(String(texto ?? '').replace(/^\s+/, ''));
        return m ? { nome: U.norm(m[1]), resto: (m[2] ?? '').trim(), temEspaco: /^\/\S*\s/.test(String(texto).replace(/^\s+/, '')) } : null;
    };

    /**
     * Sugestões para o que foi digitado até agora.
     * @returns {{rotulo: string, completar: string, desc: string, cmd: string, arg?: string, fecha: boolean}[]}
     *   `fecha`: o texto de `completar` já é um comando completo (Enter executa).
     */
    function sugerir(texto, skills = []) {
        const p = dividir(texto);
        if (!p || String(texto).includes('\n')) return [];
        const exato = POR_NOME.get(p.nome);
        if (exato && p.temEspaco) {
            if (!exato.args) return [];
            const alvo = U.norm(p.resto);
            return exato.args.filter(a => a.startsWith(alvo)).map(a => ({
                rotulo: `/${exato.nome} ${a}`, completar: `/${exato.nome} ${a}`, desc: exato.desc, cmd: exato.nome, arg: a, fecha: true }));
        }
        const aux = p.temEspaco ? [] : skills.filter(s => s.enabled && s.id.startsWith(p.nome)).map(s => ({
            rotulo: `/${s.id} …`, completar: `/${s.id} `, desc: s.description, skill: s.id, fecha: false }));
        return [...COMANDOS.filter(c => c.nome.startsWith(p.nome)).map(c => ({
            rotulo: `/${c.nome}${c.args || c.livre ? ' …' : ''}`,
            completar: `/${c.nome}${c.args || c.livre ? ' ' : ''}`,
            desc: c.desc, cmd: c.nome, fecha: !c.args && !c.livre })), ...aux];
    }

    /**
     * `{cmd, arg}` se o texto é um comando válido; `{cmd, invalido: true}` se o comando existe mas o
     * argumento não; `{desconhecido: true}` se começa com "/" e nada casa; `null` se é texto comum.
     * Caminho de arquivo ("/home/x") e texto com várias linhas não são comando.
     */
    function interpretar(texto) {
        const bruto = String(texto ?? '');
        const skill = /^\s*\/([a-z0-9][a-z0-9_-]{0,47}:[a-z0-9]+(?:-[a-z0-9]+)*)(?:\s+([\s\S]*))?$/.exec(bruto);
        if (skill) return { skill: skill[1], arg: (skill[2] || '').trim() };
        if (bruto.includes('\n') || !/^\s*\/[a-zà-ÿ]*(\s|$)/i.test(bruto)) return null;
        const p = dividir(bruto);
        if (!p) return null;
        const c = POR_NOME.get(p.nome);
        if (!c) return { desconhecido: true, nome: p.nome };
        if (c.args) {
            const arg = U.norm(p.resto);
            return c.args.includes(arg) ? { cmd: c.nome, arg } : { cmd: c.nome, invalido: true, opcoes: c.args };
        }
        if (c.livre) return p.resto ? { cmd: c.nome, arg: p.resto } : { cmd: c.nome, invalido: true, opcoes: [c.livre] };
        return { cmd: c.nome };
    }

    return { COMANDOS, sugerir, interpretar };
});
