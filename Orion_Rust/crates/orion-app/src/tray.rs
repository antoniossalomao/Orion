//! Bandeja do Windows, atalhos globais e janela escondida: o app continua rodando em segundo plano.
//! Fora do Windows não há bandeja (`Bandeja::nova` devolve `None` e fechar a janela encerra o app).
pub use orion_platform::DesktopAction as Acao;

pub struct Bandeja {
    #[cfg(windows)]
    shell: orion_platform::NativeShell,
}

impl Bandeja {
    pub fn nova() -> Option<Self> {
        #[cfg(windows)]
        {
            orion_platform::NativeShell::new().ok().map(|shell| Bandeja { shell })
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    pub fn consultar(&self) -> Vec<Acao> {
        #[cfg(windows)]
        {
            self.shell.poll()
        }
        #[cfg(not(windows))]
        {
            vec![]
        }
    }
}
