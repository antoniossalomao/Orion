//! Identidade de pacote esparso (MSIX) com certificado autoassinado.
//!
//! O que isto faz: gera um certificado `CN=Orion` autoassinado, assina um pacote esparso que aponta
//! para a pasta do app (`-ExternalLocation`) e o registra no Windows. Isso dá ao `orion.exe` uma
//! identidade de pacote (OCR do Windows, avisos). Exige o SDK do Windows (`makeappx`/`signtool`) e
//! administrador para confiar no certificado; se faltar um dos dois, o passo é **pulado e dito**, e
//! a instalação segue sem identidade. Nada aqui foi executado num Windows real.

use image::ImageEncoder;
use std::path::Path;

pub const PACKAGE_NAME: &str = "Orion.Desktop";
pub const PUBLISHER: &str = "CN=Orion";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityOutcome {
    /// Pacote registrado; o certificado fica identificado pelo thumbprint para a desinstalação.
    Registered { thumbprint: String },
    /// Não deu para registrar (SDK ausente, sem administrador). O motivo vai para a tela.
    Skipped(String),
}

/// Manifesto do pacote esparso. `version` precisa ter quatro números (`0.1.0.0`).
pub fn appx_manifest(version: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10" xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10" xmlns:uap10="http://schemas.microsoft.com/appx/manifest/uap/windows10/10" xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities" IgnorableNamespaces="uap uap10 rescap">
  <Identity Name="{PACKAGE_NAME}" Publisher="{PUBLISHER}" Version="{version}" ProcessorArchitecture="x64" />
  <Properties>
    <DisplayName>Orion</DisplayName>
    <PublisherDisplayName>Orion</PublisherDisplayName>
    <Logo>Assets\Square150.png</Logo>
    <uap10:AllowExternalContent>true</uap10:AllowExternalContent>
  </Properties>
  <Resources><Resource Language="pt-br" /></Resources>
  <Dependencies><TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.19041.0" MaxVersionTested="10.0.26100.0" /></Dependencies>
  <Capabilities><rescap:Capability Name="runFullTrust" /></Capabilities>
  <Applications>
    <Application Id="Orion" Executable="orion.exe" EntryPoint="Windows.FullTrustApplication" uap10:TrustLevel="mediumIL" uap10:RuntimeBehavior="win32App">
      <uap:VisualElements DisplayName="Orion" Description="Orion" BackgroundColor="transparent" Square150x150Logo="Assets\Square150.png" Square44x44Logo="Assets\Square44.png" />
    </Application>
  </Applications>
</Package>
"#
    )
}

/// `0.1.0` vira `0.1.0.0`; qualquer coisa fora de números separados por ponto cai em `0.0.0.0`.
pub fn four_part_version(v: &str) -> String {
    let parts: Vec<u16> = v.split('.').filter_map(|p| p.parse().ok()).collect();
    if parts.is_empty() || parts.len() > 4 || v.split('.').count() != parts.len() {
        return "0.0.0.0".into();
    }
    (0..4).map(|i| parts.get(i).copied().unwrap_or(0).to_string()).collect::<Vec<_>>().join(".")
}

fn png(size: u32) -> Vec<u8> {
    let rgba = crate::sky::generate_belt_icon_rgba(size);
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out).write_image(&rgba, size, size, image::ExtendedColorType::Rgba8).expect("PNG em memória");
    out
}

/// Monta a pasta do pacote (manifesto e logos) em `dir`.
pub fn write_package_dir(dir: &Path, version: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir.join("Assets"))?;
    std::fs::write(dir.join("AppxManifest.xml"), appx_manifest(&four_part_version(version)))?;
    std::fs::write(dir.join("Assets").join("Square150.png"), png(150))?;
    std::fs::write(dir.join("Assets").join("Square44.png"), png(44))?;
    Ok(())
}

/// Thumbprint de certificado: 40 hexadecimais. Qualquer outra coisa é recusada antes de ir a um script.
pub fn valid_thumbprint(t: &str) -> bool {
    t.len() == 40 && t.chars().all(|c| c.is_ascii_hexdigit())
}

/// Lê a última linha útil da saída do script: `OK:<thumbprint>` ou `SKIP:<motivo>`.
pub fn parse_script_output(stdout: &str) -> Result<IdentityOutcome, String> {
    let line = stdout.lines().map(str::trim).rfind(|l| !l.is_empty()).ok_or("o script não respondeu")?;
    if let Some(t) = line.strip_prefix("OK:") {
        return if valid_thumbprint(t) { Ok(IdentityOutcome::Registered { thumbprint: t.to_uppercase() }) } else { Err(format!("thumbprint inválido: {t}")) };
    }
    if let Some(motivo) = line.strip_prefix("SKIP:") {
        return Ok(IdentityOutcome::Skipped(motivo.to_string()));
    }
    Err(format!("resposta inesperada do script: {line}"))
}

pub const REGISTER_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$sdk = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Directory -ErrorAction SilentlyContinue |
  Where-Object { (Test-Path "$($_.FullName)\x64\makeappx.exe") -and (Test-Path "$($_.FullName)\x64\signtool.exe") } |
  Sort-Object Name -Descending | Select-Object -First 1
if (-not $sdk) { 'SKIP:SDK do Windows (makeappx e signtool) não encontrado'; exit 0 }
$bin = "$($sdk.FullName)\x64"
$cert = $null
try {
  $cert = New-SelfSignedCertificate -Type Custom -Subject 'CN=Orion' -KeyUsage DigitalSignature -FriendlyName 'Orion' `
    -CertStoreLocation 'Cert:\CurrentUser\My' -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
  $pw = ConvertTo-SecureString -String $env:ORION_PFX_PASSWORD -Force -AsPlainText
  Export-PfxCertificate -Cert $cert -FilePath $env:ORION_PFX -Password $pw | Out-Null
  Export-Certificate -Cert $cert -FilePath $env:ORION_CER | Out-Null
  try { Import-Certificate -FilePath $env:ORION_CER -CertStoreLocation 'Cert:\LocalMachine\TrustedPeople' | Out-Null }
  catch { Remove-Item "Cert:\CurrentUser\My\$($cert.Thumbprint)" -ErrorAction SilentlyContinue; 'SKIP:sem permissão de administrador para confiar no certificado'; exit 0 }
  & "$bin\makeappx.exe" pack /d $env:ORION_PKGDIR /p $env:ORION_MSIX /nv | Out-Null
  if ($LASTEXITCODE -ne 0) { throw 'makeappx falhou' }
  & "$bin\signtool.exe" sign /fd SHA256 /f $env:ORION_PFX /p $env:ORION_PFX_PASSWORD $env:ORION_MSIX | Out-Null
  if ($LASTEXITCODE -ne 0) { throw 'signtool falhou' }
  Add-AppxPackage -Path $env:ORION_MSIX -ExternalLocation $env:ORION_APPDIR
  "OK:$($cert.Thumbprint)"
} catch {
  if ($cert) {
    Remove-Item "Cert:\CurrentUser\My\$($cert.Thumbprint)" -ErrorAction SilentlyContinue
    Remove-Item "Cert:\LocalMachine\TrustedPeople\$($cert.Thumbprint)" -ErrorAction SilentlyContinue
  }
  throw
}
"#;

pub const UNREGISTER_SCRIPT: &str = r#"
$ErrorActionPreference = 'Continue'
Get-AppxPackage -Name 'Orion.Desktop' | Remove-AppxPackage
Remove-Item "Cert:\LocalMachine\TrustedPeople\$env:ORION_THUMBPRINT" -ErrorAction SilentlyContinue
Remove-Item "Cert:\CurrentUser\My\$env:ORION_THUMBPRINT" -ErrorAction SilentlyContinue
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifesto_tem_identidade_e_conteudo_externo() {
        let m = appx_manifest("0.1.0.0");
        assert!(m.contains(r#"Name="Orion.Desktop""#) && m.contains(r#"Publisher="CN=Orion""#));
        assert!(m.contains("AllowExternalContent>true<"));
        assert!(m.contains(r#"Executable="orion.exe""#));
    }

    #[test]
    fn versao_vira_quatro_numeros() {
        assert_eq!(four_part_version("0.1.0"), "0.1.0.0");
        assert_eq!(four_part_version("1.2"), "1.2.0.0");
        assert_eq!(four_part_version("1.2.3.4"), "1.2.3.4");
        for ruim in ["", "x", "1.2.3.4.5", "1..2", "1.a"] {
            assert_eq!(four_part_version(ruim), "0.0.0.0", "{ruim:?}");
        }
    }

    #[test]
    fn pasta_do_pacote_tem_manifesto_e_logos_png() {
        let tmp = tempfile::tempdir().unwrap();
        write_package_dir(tmp.path(), "0.1.0").unwrap();
        assert!(std::fs::read_to_string(tmp.path().join("AppxManifest.xml")).unwrap().contains("Version=\"0.1.0.0\""));
        for logo in ["Square150.png", "Square44.png"] {
            let bytes = std::fs::read(tmp.path().join("Assets").join(logo)).unwrap();
            assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "{logo}");
        }
    }

    #[test]
    fn saida_do_script_e_lida_com_cuidado() {
        let tp = "A".repeat(40);
        assert_eq!(parse_script_output(&format!("aviso\nOK:{tp}\n")).unwrap(), IdentityOutcome::Registered { thumbprint: tp.clone() });
        assert_eq!(parse_script_output("SKIP:sem SDK").unwrap(), IdentityOutcome::Skipped("sem SDK".into()));
        for ruim in ["", "OK:123", "OK:../../x", "qualquer coisa"] {
            assert!(parse_script_output(ruim).is_err(), "{ruim:?}");
        }
        assert!(valid_thumbprint(&tp) && !valid_thumbprint("zz") && !valid_thumbprint(&format!("{}Z", "A".repeat(39))));
    }
}
