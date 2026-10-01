// shell.ts — contrato fino entre o frontend Svelte e a casca nativa que o
// hospeda (Tauri/Rust). Ver LYRA_TECNICO.md §9.6:
// "o Svelte só chama a interface, nunca a implementação" — sem isso, cada
// componente teria que saber em qual casca está rodando.
//
// Duas implementações: `browser` (fallback — `npm run dev` puro ou servido
// pelo backend sem casca) e `tauri` (comandos Rust via `invoke`).

export interface LyraShell {
	/** Nome da casca atual — só pra log/debug, componentes não devem ramificar nisso. */
	readonly kind: 'browser' | 'tauri';

	/** Lê um segredo (API key etc.) guardado fora do webview/localStorage.
	 * Casca browser não tem onde guardar com segurança — sempre retorna null. */
	getSecret(id: string): Promise<string | null>;

	/** Persiste uma configuração no lado nativo, não no frontend. */
	setSetting(key: string, value: unknown): Promise<void>;

	/** Garante que o backend (cerebro_maestro) está de pé; casca browser
	 * assume que alguém já subiu o processo por fora. */
	startBackend(): Promise<void>;
}

class BrowserShell implements LyraShell {
	readonly kind = 'browser' as const;

	async getSecret(): Promise<string | null> {
		return null;
	}

	async setSetting(): Promise<void> {
		// sem lado nativo — no-op deliberado, não há onde persistir com segurança
	}

	async startBackend(): Promise<void> {
		// sem lado nativo — assume que o backend já está rodando (dev/`/ui`)
	}
}

/**
 * Casca Tauri — detectada por `window.__TAURI_INTERNALS__`, injetado
 * automaticamente pelo runtime Tauri em toda janela. Chama os comandos Rust
 * via `invoke` — implementação em `src-tauri/src/main.rs`.
 */
class TauriShell implements LyraShell {
	readonly kind = 'tauri' as const;

	async getSecret(id: string): Promise<string | null> {
		const { invoke } = await import('@tauri-apps/api/core');
		return invoke<string | null>('get_secret', { id });
	}

	async setSetting(key: string, value: unknown): Promise<void> {
		const { invoke } = await import('@tauri-apps/api/core');
		await invoke('set_setting', { key, value });
	}

	async startBackend(): Promise<void> {
		const { invoke } = await import('@tauri-apps/api/core');
		await invoke('start_backend');
	}
}

/** Detecta a casca atual e devolve a implementação certa.
 * `browser` é o fallback (dev puro, ou `/ui` aberto direto sem casca por
 * perto). */
function detectarCasca(): LyraShell {
	if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
		return new TauriShell();
	}
	return new BrowserShell();
}

export const shell: LyraShell = detectarCasca();
