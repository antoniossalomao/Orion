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
        use windows::{
            core::HSTRING,
            Security::Credentials::UI::{UserConsentVerificationResult, UserConsentVerifier},
        };
        let result = UserConsentVerifier::RequestVerificationAsync(&HSTRING::from("Entrar no Orion"))
            .and_then(|op| op.get())
            .map_err(|_| "Não foi possível abrir o Windows Hello.".to_string())?;
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
