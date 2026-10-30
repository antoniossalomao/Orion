//! F12.04: acorda o PC suspenso para um agendamento ou lembrete. Uma tarefa só, `Orion\Despertar`,
//! no Agendador do Windows do seu usuário, com "Despertar o computador para executar" ligado.
//! A tarefa não faz nada além de acordar (`cmd /c exit 0`): quem roda o agendamento é o Orion.
//! Precisa dos "timers de despertar" permitidos no plano de energia do Windows.

/// Nome da tarefa no Agendador.
pub const TAREFA: &str = "Orion\\Despertar";

/// XML da tarefa para `quando` (hora local, `AAAA-MM-DDTHH:MM:SS`).
pub fn xml(quando: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Acorda o PC para o próximo agendamento do Orion.</Description></RegistrationInfo>
  <Triggers><TimeTrigger><StartBoundary>{quando}</StartBoundary><Enabled>true</Enabled></TimeTrigger></Triggers>
  <Principals><Principal id="Author"><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
  <Settings>
    <WakeToRun>true</WakeToRun>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <StartWhenAvailable>false</StartWhenAvailable>
    <Hidden>true</Hidden>
    <ExecutionTimeLimit>PT1M</ExecutionTimeLimit>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
  </Settings>
  <Actions Context="Author"><Exec><Command>cmd.exe</Command><Arguments>/c exit 0</Arguments></Exec></Actions>
</Task>
"#
    )
}

/// Hora local válida para o XML (só dígitos, `-`, `:` e `T`, no formato exato).
fn hora_valida(q: &str) -> bool {
    q.len() == 19 && q.chars().enumerate().all(|(i, c)| match i {
        4 | 7 => c == '-',
        10 => c == 'T',
        13 | 16 => c == ':',
        _ => c.is_ascii_digit(),
    })
}

/// Registra (ou troca) a tarefa para `quando`; `None` apaga a tarefa.
#[cfg(windows)]
pub fn agendar(quando: Option<&str>) -> anyhow::Result<()> {
    use std::os::windows::process::CommandExt;
    const SEM_JANELA: u32 = 0x0800_0000;
    let rodar = |args: &[&str]| -> anyhow::Result<bool> {
        let s = std::process::Command::new("schtasks").args(args).creation_flags(SEM_JANELA).output()?;
        Ok(s.status.success())
    };
    let Some(q) = quando else {
        // apagar uma tarefa que não existe não é erro
        let _ = rodar(&["/Delete", "/TN", TAREFA, "/F"])?;
        return Ok(());
    };
    anyhow::ensure!(hora_valida(q), "hora inválida para o Agendador: {q}");
    // o schtasks lê o XML em UTF-16 com BOM
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(xml(q).encode_utf16().flat_map(u16::to_le_bytes));
    let arq = std::env::temp_dir().join(format!("orion-despertar-{}.xml", std::process::id()));
    std::fs::write(&arq, bytes)?;
    let ok = rodar(&["/Create", "/TN", TAREFA, "/XML", &arq.to_string_lossy(), "/F"]);
    let _ = std::fs::remove_file(&arq);
    anyhow::ensure!(ok?, "o Agendador do Windows recusou a tarefa de despertar");
    Ok(())
}

#[cfg(not(windows))]
pub fn agendar(quando: Option<&str>) -> anyhow::Result<()> {
    if let Some(q) = quando {
        anyhow::ensure!(hora_valida(q), "hora inválida para o Agendador: {q}");
    }
    Ok(())
}
