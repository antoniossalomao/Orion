use super::*;
use std::collections::HashMap;
use std::io::Cursor;

struct Fake(HashMap<String, Vec<u8>>);

impl Fetch for Fake {
    fn get(&self, url: &str, _max: u64) -> Result<Vec<u8>> {
        self.0.get(url).cloned().ok_or_else(|| UpdateError::Fetch(format!("404 {url}")))
    }
}

struct Cenario {
    dir: tempfile::TempDir,
    config: UpdateConfig,
    fetch: Fake,
    sk: minisign::SecretKey,
    pk: minisign::PublicKey,
}

const FEED_URL: &str = "https://exemplo.test/latest.json";

fn assinar(sk: &minisign::SecretKey, pk: &minisign::PublicKey, data: &[u8]) -> Vec<u8> {
    minisign::sign(Some(pk), sk, Cursor::new(data), Some("trusted"), Some("untrusted")).unwrap().to_string().into_bytes()
}

fn feed_json(version: &str, exe: &[u8]) -> Vec<u8> {
    serde_json::to_vec(&Feed {
        version: version.into(),
        channel: "stable".into(),
        notes: String::new(),
        files: vec![FeedFile { name: "orion.exe".into(), url: "https://exemplo.test/orion.exe".into(), sha256: hex::encode(Sha256::digest(exe)), bytes: exe.len() as u64 }],
    })
    .unwrap()
}

fn cenario(instalada: &str, nova: &str, exe_novo: &[u8]) -> Cenario {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("orion.exe"), b"v-antigo").unwrap();
    let m = serde_json::json!({"version": instalada, "files": [{"name": "orion.exe", "sha256": hex::encode(Sha256::digest(b"v-antigo"))}]});
    std::fs::write(dir.path().join(MANIFEST), serde_json::to_vec(&m).unwrap()).unwrap();
    let minisign::KeyPair { pk, sk } = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let feed = feed_json(nova, exe_novo);
    let mut web = HashMap::new();
    web.insert(format!("{FEED_URL}.minisig"), assinar(&sk, &pk, &feed));
    web.insert(FEED_URL.to_string(), feed);
    web.insert("https://exemplo.test/orion.exe".into(), exe_novo.to_vec());
    let config = UpdateConfig { channel: "stable".into(), auto_update: true, feed_url: Some(FEED_URL.into()), public_key: Some(pk.to_box().unwrap().to_string()), ..Default::default() };
    Cenario { dir, config, fetch: Fake(web), sk, pk }
}

#[test]
fn atualiza_quando_assinatura_e_hash_conferem_e_guarda_o_antigo() {
    let c = cenario("0.1.0", "0.2.0", b"v-novo");
    assert_eq!(check(&c.config, c.dir.path(), &c.fetch, false).unwrap(), Outcome::Updated("0.2.0".into()));
    assert_eq!(std::fs::read(c.dir.path().join("orion.exe")).unwrap(), b"v-novo");
    assert_eq!(std::fs::read(c.dir.path().join("orion.exe.bak")).unwrap(), b"v-antigo");
    let m: serde_json::Value = serde_json::from_slice(&std::fs::read(c.dir.path().join(MANIFEST)).unwrap()).unwrap();
    assert_eq!(m["version"], "0.2.0");
    let novidade = pending_whats_new(c.dir.path()).unwrap();
    assert_eq!((novidade.version.as_str(), novidade.previous_version.as_str()), ("0.2.0", "0.1.0"));
    dismiss_whats_new(c.dir.path());
    assert!(pending_whats_new(c.dir.path()).is_none());
    assert_eq!(m["files"][0]["sha256"], hex::encode(Sha256::digest(b"v-novo")));
    assert!(!c.dir.path().join(".update-0.2.0").exists(), "staging limpo");
}

#[test]
fn sem_auto_update_so_avisa() {
    let mut c = cenario("0.1.0", "0.2.0", b"v-novo");
    c.config.auto_update = false;
    assert_eq!(check(&c.config, c.dir.path(), &c.fetch, false).unwrap(), Outcome::Available("0.2.0".into()));
    assert_eq!(std::fs::read(c.dir.path().join("orion.exe")).unwrap(), b"v-antigo");
    assert_eq!(check(&c.config, c.dir.path(), &c.fetch, true).unwrap(), Outcome::Updated("0.2.0".into()));
}

#[test]
fn recusa_versao_igual_ou_menor() {
    for instalada in ["0.2.0", "0.3.0", "0.2"] {
        let c = cenario(instalada, "0.2.0", b"v-novo");
        assert_eq!(check(&c.config, c.dir.path(), &c.fetch, true).unwrap(), Outcome::UpToDate, "{instalada}");
        assert_eq!(std::fs::read(c.dir.path().join("orion.exe")).unwrap(), b"v-antigo");
    }
}

#[test]
fn feed_adulterado_ou_assinado_por_outra_chave_nao_passa() {
    let mut c = cenario("0.1.0", "0.2.0", b"v-novo");
    let adulterado = feed_json("9.9.9", b"v-novo");
    c.fetch.0.insert(FEED_URL.into(), adulterado);
    assert!(matches!(check(&c.config, c.dir.path(), &c.fetch, true), Err(UpdateError::BadSignature(_))));

    let mut c = cenario("0.1.0", "0.2.0", b"v-novo");
    let minisign::KeyPair { pk: outro_pk, sk: outro_sk } = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let feed = c.fetch.0[FEED_URL].clone();
    c.fetch.0.insert(format!("{FEED_URL}.minisig"), assinar(&outro_sk, &outro_pk, &feed));
    assert!(matches!(check(&c.config, c.dir.path(), &c.fetch, true), Err(UpdateError::BadSignature(_))));
    assert_eq!(std::fs::read(c.dir.path().join("orion.exe")).unwrap(), b"v-antigo");
    let _ = (&c.sk, &c.pk);
}

#[test]
fn arquivo_com_hash_errado_nao_troca_nada() {
    let mut c = cenario("0.1.0", "0.2.0", b"v-novo");
    c.fetch.0.insert("https://exemplo.test/orion.exe".into(), b"v-troca".to_vec());
    assert!(matches!(check(&c.config, c.dir.path(), &c.fetch, true), Err(UpdateError::BadHash(_))));
    assert_eq!(std::fs::read(c.dir.path().join("orion.exe")).unwrap(), b"v-antigo");
    assert!(!c.dir.path().join("orion.exe.bak").exists());
    assert!(!c.dir.path().join(".update-0.2.0").exists());
}

#[test]
fn sem_chave_ou_sem_feed_nao_faz_nada() {
    let mut c = cenario("0.1.0", "0.2.0", b"v-novo");
    c.config.public_key = None;
    assert!(matches!(check(&c.config, c.dir.path(), &c.fetch, true), Err(UpdateError::NotConfigured(_))));
    c.config.public_key = Some("x".into());
    c.config.feed_url = None;
    assert!(matches!(check(&c.config, c.dir.path(), &c.fetch, true), Err(UpdateError::NotConfigured(_))));
}

#[test]
fn feed_assinado_nao_pode_mexer_em_arquivo_fora_da_lista_nem_em_http() {
    let c = cenario("0.1.0", "0.2.0", b"v-novo");
    let pk = c.config.public_key.clone().unwrap();
    let mut f: Feed = serde_json::from_slice(&c.fetch.0[FEED_URL]).unwrap();
    for (nome, url) in [("../fora.exe", "https://x/a"), ("Orion Setup.exe", "https://x/a"), ("orion.exe", "http://x/a")] {
        f.files[0].name = nome.into();
        f.files[0].url = url.into();
        let bytes = serde_json::to_vec(&f).unwrap();
        let sig = String::from_utf8(assinar(&c.sk, &c.pk, &bytes)).unwrap();
        assert!(matches!(verify_feed(&bytes, &sig, &pk), Err(UpdateError::BadFeed(_))), "{nome} {url}");
    }
}

#[test]
fn rollback_devolve_o_antigo_e_a_versao() {
    let c = cenario("0.1.0", "0.2.0", b"v-novo");
    check(&c.config, c.dir.path(), &c.fetch, true).unwrap();
    assert_eq!(rollback(c.dir.path()).unwrap(), 1);
    assert_eq!(std::fs::read(c.dir.path().join("orion.exe")).unwrap(), b"v-antigo");
    let m: serde_json::Value = serde_json::from_slice(&std::fs::read(c.dir.path().join(MANIFEST)).unwrap()).unwrap();
    assert_eq!(m["version"], "0.1.0");
    assert_eq!(m["files"][0]["sha256"], hex::encode(Sha256::digest(b"v-antigo")));
    assert_eq!(rollback(c.dir.path()).unwrap(), 0);
}

#[test]
fn comparacao_de_versao() {
    assert!(is_newer("0.2.0", "0.1.9") && is_newer("1.0", "0.9.9.9") && is_newer("0.1.0.1", "0.1.0"));
    assert!(!is_newer("0.1.0", "0.1") && !is_newer("x", "0.1") && !is_newer("0.2", "x"));
}

#[test]
fn baixar_release_confere_assinatura_e_hash_e_so_grava_no_destino() {
    let c = cenario("0.1.0", "0.2.0", b"v-novo");
    let destino = tempfile::tempdir().unwrap();
    let chave = c.config.public_key.clone().unwrap();
    let feed = baixar_release(FEED_URL, &chave, &c.fetch, destino.path(), &|_, _| {}).unwrap();
    assert_eq!(feed.version, "0.2.0");
    assert_eq!(std::fs::read(destino.path().join("orion.exe")).unwrap(), b"v-novo");
    // a pasta do app (instalada) não foi tocada
    assert_eq!(std::fs::read(c.dir.path().join("orion.exe")).unwrap(), b"v-antigo");
    // sem chave: recusa
    assert!(matches!(baixar_release(FEED_URL, " ", &c.fetch, destino.path(), &|_, _| {}), Err(UpdateError::NotConfigured(_))));
    // hash errado: recusa
    let mut web = c.fetch.0.clone();
    web.insert("https://exemplo.test/orion.exe".into(), b"adulterado".to_vec());
    let d2 = tempfile::tempdir().unwrap();
    assert!(matches!(baixar_release(FEED_URL, &chave, &Fake(web), d2.path(), &|_, _| {}), Err(UpdateError::BadHash(_))));
    assert!(!d2.path().join("orion.exe").exists());
}
