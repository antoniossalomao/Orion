//! OAuth do Google para backup: navegador do sistema, PKCE e callback só em loopback.
//! Tokens ficam no cofre, nunca na URL de retorno da UI nem em log.
use anyhow::{ensure, Context};
use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub async fn connect(body: Value) -> anyhow::Result<Value> {
    let client_id = body["client_id"].as_str().unwrap_or("");
    let client_secret = zeroize::Zeroizing::new(body["client_secret"].as_str().unwrap_or("").to_string());
    ensure!(
        client_id.ends_with(".apps.googleusercontent.com") && client_id.len() < 512,
        "informe o ID de um cliente OAuth de aplicativo para computador do Google"
    );
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let redirect = format!("http://127.0.0.1:{}/oauth/google", listener.local_addr()?.port());
    let verifier = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    let state = uuid::Uuid::new_v4().simple().to_string();
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut auth = url::Url::parse("https://accounts.google.com/o/oauth2/v2/auth")?;
    auth.query_pairs_mut().extend_pairs([
        ("client_id", client_id),
        ("redirect_uri", &redirect),
        ("response_type", "code"),
        ("scope", "https://www.googleapis.com/auth/drive.file"),
        ("access_type", "offline"),
        ("prompt", "consent"),
        ("state", &state),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
    ]);
    let url = auth.to_string();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        orion_app::onboarding::system_command("rundll32.exe")
            .creation_flags(0x08000000)
            .args(["url.dll,FileProtocolHandler", &url])
            .spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        orion_app::onboarding::system_command("xdg-open").arg(&url).spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        orion_app::onboarding::system_command("open").arg(&url).spawn()?;
    }
    let code = tokio::time::timeout(std::time::Duration::from_secs(180), async {
        loop {
            let (mut stream, _) = listener.accept().await?;
            let mut bytes = vec![0u8; 8192];
            let n = tokio::time::timeout(std::time::Duration::from_secs(5), stream.read(&mut bytes)).await??;
            let request = String::from_utf8_lossy(&bytes[..n]);
            let path = request
                .lines()
                .next()
                .and_then(|l| l.strip_prefix("GET "))
                .and_then(|l| l.split_once(' ').map(|p| p.0))
                .unwrap_or("");
            let parsed = url::Url::parse(&format!("http://127.0.0.1{path}"))?;
            let args = parsed.query_pairs().into_owned().collect::<std::collections::HashMap<_, _>>();
            let valid = parsed.path() == "/oauth/google" && args.get("state") == Some(&state);
            let text = if valid {
                "Orion: pode fechar esta aba e voltar ao aplicativo."
            } else {
                "Solicitacao recusada."
            };
            let reply = format!(
                "HTTP/1.1 {}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                if valid { "200 OK" } else { "400 Bad Request" },
                text.len(),
                text
            );
            stream.write_all(reply.as_bytes()).await?;
            if valid {
                ensure!(!args.contains_key("error"), "autorização Google cancelada");
                break Ok::<String, anyhow::Error>(args.get("code").context("Google não devolveu código de autorização")?.clone());
            }
        }
    })
    .await
    .context("a conexão com o Google expirou; tente novamente")??;
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let response = http
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", client_id),
            ("client_secret", &client_secret),
            ("redirect_uri", &redirect),
            ("code", &code),
            ("code_verifier", &verifier),
        ])
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("não foi possível concluir a conexão com o Google"))?;
    ensure!(response.status().is_success(), "o Google recusou a autorização; confira o cliente OAuth");
    let mut tokens: Value = response.json().await?;
    let refresh = zeroize::Zeroizing::new(
        tokens["refresh_token"]
            .as_str()
            .context("Google não autorizou acesso offline; conecte novamente")?
            .to_string(),
    );
    tokens = Value::Null;
    let _ = tokens;
    // Um único registro evita cliente e token de contas diferentes em gravação parcial.
    let credential = zeroize::Zeroizing::new(json!({"client_id":client_id,"client_secret":&*client_secret,"refresh_token":&*refresh}).to_string());
    orion_ai::secrets::set_secret("ORION_DRIVE_OAUTH", &credential).map_err(|_| anyhow::anyhow!("não foi possível guardar a autorização no cofre do sistema"))?;
    Ok(json!({"ok":true,"connected":true}))
}

pub async fn upload(path: &std::path::Path) -> anyhow::Result<()> {
    let credential = zeroize::Zeroizing::new(orion_ai::secrets::get_secret("ORION_DRIVE_OAUTH").context("Google Drive não conectado")?);
    let credential: Value = serde_json::from_str(&credential)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let response = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", credential["client_id"].as_str().unwrap_or("")),
            ("client_secret", credential["client_secret"].as_str().unwrap_or("")),
            ("refresh_token", credential["refresh_token"].as_str().unwrap_or("")),
        ])
        .send()
        .await?;
    ensure!(response.status().is_success(), "renove a conexão com o Google Drive");
    let token: Value = response.json().await?;
    let access = token["access_token"].as_str().context("Google não devolveu token de acesso")?;
    let installation = format!(
        "{:x}",
        Sha256::digest(path.parent().unwrap_or_else(|| std::path::Path::new("")).to_string_lossy().as_bytes())
    );
    let query =
        format!("trashed = false and appProperties has {{ key='orion_backup' and value='v1' }} and appProperties has {{ key='orion_installation' and value='{installation}' }}");
    let listed: Value = client
        .get("https://www.googleapis.com/drive/v3/files")
        .bearer_auth(access)
        .query(&[("q", query.as_str()), ("orderBy", "createdTime desc"), ("fields", "files(id,name)"), ("pageSize", "100")])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    if listed["files"].as_array().into_iter().flatten().any(|f| f["name"].as_str() == Some(name.as_ref())) {
        return Ok(());
    }
    let bytes = tokio::fs::read(path).await?;
    let meta = json!({"name":name,"appProperties":{"orion_backup":"v1","orion_installation":installation}});
    let boundary = format!("orion-{}", uuid::Uuid::new_v4().simple());
    let mut body =
        format!("--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n--{boundary}\r\nContent-Type: application/octet-stream\r\n\r\n").into_bytes();
    body.extend_from_slice(&bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let response = client
        .post("https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart")
        .bearer_auth(access)
        .header("Content-Type", format!("multipart/related; boundary={boundary}"))
        .body(body)
        .send()
        .await?;
    ensure!(response.status().is_success(), "Google Drive recusou o backup");
    // A cópia nova soma uma às anteriores. Só arquivos deste Orion entram na retenção.
    for file in listed["files"].as_array().into_iter().flatten().skip(6) {
        if let Some(id) = file["id"].as_str().filter(|s| s.chars().all(|c| c.is_ascii_alphanumeric() || "_-".contains(c))) {
            client
                .delete(format!("https://www.googleapis.com/drive/v3/files/{id}"))
                .bearer_auth(access)
                .send()
                .await?
                .error_for_status()?;
        }
    }
    Ok(())
}
