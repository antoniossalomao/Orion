//! Elevação pelo UAC só para instalar/desinstalar, antes de qualquer alteração no sistema.
//! Prévia e notas de versão não exigem privilégios (REGRAS.md: ação de alto risco exige confirmação explícita).

#[cfg(windows)]
pub fn relaunch_if_needed() -> bool {
    use std::ffi::{c_void, OsStr, OsString};
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "shell32")]
    extern "system" {
        fn IsUserAnAdmin() -> i32;
        fn ShellExecuteW(hwnd: *mut c_void, operation: *const u16, file: *const u16, parameters: *const u16, directory: *const u16, show: i32) -> isize;
    }
    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(hwnd: *mut c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }

    fn wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(Some(0)).collect()
    }

    // Argumentos continuam dados, inclusive com espaços, aspas e barras finais.
    fn quoted(value: &OsStr) -> Vec<u16> {
        let mut out = vec![b'"' as u16];
        let mut slashes = 0;
        for c in value.encode_wide() {
            if c == b'\\' as u16 {
                slashes += 1;
                continue;
            }
            out.extend(std::iter::repeat_n(b'\\' as u16, if c == b'"' as u16 { slashes * 2 + 1 } else { slashes }));
            out.push(c);
            slashes = 0;
        }
        out.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
        out.push(b'"' as u16);
        out
    }

    const SE_ERR_ACCESSDENIED: isize = 5;
    const ERROR_CANCELLED: i32 = 1223;
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let has = |arg: &str| args.iter().any(|a| a == arg);
    let install = !has("--whats-new") && !has("--preview") && !has("--prototype-hw");
    if (!install && !has("--uninstall")) || unsafe { IsUserAnAdmin() != 0 } {
        return false;
    }

    let result = (|| {
        let exe = wide(std::env::current_exe()?.as_os_str());
        let directory = wide(std::env::current_dir()?.as_os_str());
        let mut parameters = Vec::new();
        for arg in &args {
            if !parameters.is_empty() { parameters.push(b' ' as u16); }
            parameters.extend(quoted(arg));
        }
        parameters.push(0);
        let operation = wide(OsStr::new("runas"));
        let status = unsafe { ShellExecuteW(std::ptr::null_mut(), operation.as_ptr(), exe.as_ptr(), parameters.as_ptr(), directory.as_ptr(), 1) };
        if status > 32 {
            Ok(())
        } else if status == SE_ERR_ACCESSDENIED {
            // Quem cancela o prompt do UAC recebe "acesso negado" do ShellExecute (e 1223 no último erro).
            Err(std::io::Error::from_raw_os_error(ERROR_CANCELLED))
        } else {
            Err(std::io::Error::last_os_error())
        }
    })();

    if let Err(error) = result {
        let title = wide(OsStr::new("Orion Setup"));
        if error.raw_os_error() == Some(ERROR_CANCELLED) {
            // Cancelar o UAC não é falha: avisa com calma, sem copiar arquivos e sem repetir o pedido.
            let message = wide(OsStr::new("Você cancelou a permissão de administrador, então nada foi instalado nem alterado.\n\nAbra o instalador de novo quando quiser continuar."));
            unsafe { MessageBoxW(std::ptr::null_mut(), message.as_ptr(), title.as_ptr(), 0x40); }
            std::process::exit(0);
        }
        let message = wide(OsStr::new(&format!("Não foi possível abrir o instalador como administrador. Nenhum arquivo foi alterado.\n\n{error}")));
        unsafe { MessageBoxW(std::ptr::null_mut(), message.as_ptr(), title.as_ptr(), 0x10); }
        std::process::exit(1);
    }
    true
}

#[cfg(not(windows))]
pub fn relaunch_if_needed() -> bool {
    false
}
