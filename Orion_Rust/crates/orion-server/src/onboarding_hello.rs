//! Windows Hello é acionado só por botão/toggle explícito. Senha fica no Credential Manager,
//! nunca em arquivo, log, comando ou argumento de processo filho.
pub fn available() -> bool {
    #[cfg(windows)]
    {
        use windows::Security::Credentials::UI::{UserConsentVerifier, UserConsentVerifierAvailability};
        return UserConsentVerifier::CheckAvailabilityAsync()
            .and_then(|op| op.get())
            .is_ok_and(|v| v == UserConsentVerifierAvailability::Available);
    }
    #[cfg(not(windows))]
    {
        false
    }
}
pub fn verify() -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows_future::IAsyncOperation;
        use windows::{
            core::{factory, HSTRING},
            Security::Credentials::UI::{UserConsentVerificationResult, UserConsentVerifier},
            Win32::{Foundation::HWND, System::WinRT::IUserConsentVerifierInterop},
        };
        let mensagem = HSTRING::from("Entrar no Orion");
        // App desktop (sem pacote): o diálogo do Hello precisa de uma janela dona, senão pode não
        // aparecer ou abrir atrás. Com a janela do Orion usa a interop; sem ela, cai na chamada simples.
        let com_janela = orion_platform::main_window().and_then(|hwnd| {
            let interop = factory::<UserConsentVerifier, IUserConsentVerifierInterop>().ok()?;
            let op: IAsyncOperation<UserConsentVerificationResult> = unsafe { interop.RequestVerificationForWindowAsync(HWND(hwnd as *mut _), &mensagem) }.ok()?;
            op.get().ok()
        });
        let result = match com_janela {
            Some(r) => r,
            None => UserConsentVerifier::RequestVerificationAsync(&mensagem)
                .and_then(|op| op.get())
                .map_err(|_| "Não foi possível abrir o Windows Hello.".to_string())?,
        };
        return if result == UserConsentVerificationResult::Verified {
            Ok(())
        } else {
            Err("Windows Hello cancelado ou não verificado. Use sua senha.".into())
        };
    }
    #[cfg(not(windows))]
    {
        Err("Windows Hello está disponível apenas no Windows.".into())
    }
}
