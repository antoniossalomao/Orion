//! Planilhas xlsx sem dependência pesada: leitura (texto das células) e escrita só com texto
//! (célula `inlineStr`, nunca fórmula).

use quick_xml::events::Event;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;

fn ler(z: &mut zip::ZipArchive<std::fs::File>, nome: &str) -> Option<String> {
    let mut s = String::new();
    z.by_name(nome).ok()?.read_to_string(&mut s).ok()?;
    Some(s)
}

fn attr(e: &quick_xml::events::BytesStart, nome: &[u8]) -> Option<String> {
    e.attributes().flatten().find(|a| a.key.as_ref() == nome).and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
}

/// Coluna (0, 1, ...) da referência `B3`.
fn coluna(r: &str) -> usize {
    r.chars().take_while(|c| c.is_ascii_alphabetic()).fold(0usize, |n, c| n * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1)).saturating_sub(1)
}

/// (nome da planilha, linhas) das primeiras `max_planilhas`.
pub fn ler_xlsx(caminho: &Path, max_planilhas: usize, max_linhas: usize) -> anyhow::Result<Vec<(String, Vec<Vec<String>>, bool)>> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(caminho)?)?;
    let mut compartilhadas: Vec<String> = vec![];
    if let Some(xml) = ler(&mut z, "xl/sharedStrings.xml") {
        let mut r = quick_xml::Reader::from_str(&xml);
        let (mut atual, mut no_t) = (String::new(), false);
        loop {
            match r.read_event()? {
                Event::Start(e) if e.name().as_ref() == b"t" => no_t = true,
                Event::End(e) if e.name().as_ref() == b"t" => no_t = false,
                Event::Text(t) if no_t => atual.push_str(&t.unescape()?),
                Event::End(e) if e.name().as_ref() == b"si" => compartilhadas.push(std::mem::take(&mut atual)),
                Event::Eof => break,
                _ => {}
            }
        }
    }
    let mut alvos: HashMap<String, String> = HashMap::new();
    if let Some(xml) = ler(&mut z, "xl/_rels/workbook.xml.rels") {
        let mut r = quick_xml::Reader::from_str(&xml);
        loop {
            match r.read_event()? {
                Event::Empty(e) | Event::Start(e) if e.name().as_ref() == b"Relationship" => {
                    if let (Some(id), Some(t)) = (attr(&e, b"Id"), attr(&e, b"Target")) {
                        let t = t.trim_start_matches('/').to_string();
                        alvos.insert(id, if t.starts_with("xl/") { t } else { format!("xl/{t}") });
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
    }
    let mut planilhas: Vec<(String, String)> = vec![];
    let wb = ler(&mut z, "xl/workbook.xml").ok_or_else(|| anyhow::anyhow!("xlsx sem workbook"))?;
    let mut r = quick_xml::Reader::from_str(&wb);
    loop {
        match r.read_event()? {
            Event::Empty(e) | Event::Start(e) if e.name().as_ref() == b"sheet" => {
                let nome = attr(&e, b"name").unwrap_or_default();
                if let Some(alvo) = attr(&e, b"r:id").and_then(|id| alvos.get(&id).cloned()) {
                    planilhas.push((nome, alvo));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut saida = vec![];
    for (nome, alvo) in planilhas.into_iter().take(max_planilhas) {
        let Some(xml) = ler(&mut z, &alvo) else { continue };
        let mut r = quick_xml::Reader::from_str(&xml);
        let mut linhas: Vec<Vec<String>> = vec![];
        let mut linha: Vec<String> = vec![];
        let (mut tipo, mut col, mut valor, mut lendo) = (String::new(), 0usize, String::new(), false);
        let mut cortou = false;
        loop {
            match r.read_event()? {
                Event::Start(e) if e.name().as_ref() == b"c" => {
                    tipo = attr(&e, b"t").unwrap_or_default();
                    col = attr(&e, b"r").map(|x| coluna(&x)).unwrap_or(linha.len());
                    valor.clear();
                }
                Event::Start(e) if matches!(e.name().as_ref(), b"v" | b"t") => lendo = true,
                Event::Text(t) if lendo => valor.push_str(&t.unescape()?),
                Event::End(e) if matches!(e.name().as_ref(), b"v" | b"t") => lendo = false,
                Event::End(e) if e.name().as_ref() == b"c" => {
                    let texto = match tipo.as_str() {
                        "s" => valor.trim().parse::<usize>().ok().and_then(|i| compartilhadas.get(i).cloned()).unwrap_or_default(),
                        "b" => if valor.trim() == "1" { "True".into() } else { "False".into() },
                        _ => valor.clone(),
                    };
                    while linha.len() < col {
                        linha.push(String::new());
                    }
                    linha.push(texto);
                }
                Event::End(e) if e.name().as_ref() == b"row" => {
                    if linhas.len() >= max_linhas {
                        cortou = true;
                        break;
                    }
                    linhas.push(std::mem::take(&mut linha));
                }
                Event::Eof => break,
                _ => {}
            }
        }
        saida.push((nome, linhas, cortou));
    }
    Ok(saida)
}

fn esc(t: &str) -> String {
    t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn referencia(col: usize, linha: usize) -> String {
    let mut c = col + 1;
    let mut s = String::new();
    while c > 0 {
        let r = (c - 1) % 26;
        s.insert(0, (b'A' + r as u8) as char);
        c = (c - 1) / 26;
    }
    format!("{s}{}", linha + 1)
}

/// Grava uma planilha só com texto: nenhuma célula vira fórmula, mesmo começando com `=`.
pub fn gravar_xlsx(caminho: &Path, nome: &str, linhas: &[Vec<String>]) -> anyhow::Result<()> {
    let mut dados = String::new();
    for (i, l) in linhas.iter().enumerate() {
        dados += &format!("<row r=\"{}\">", i + 1);
        for (j, c) in l.iter().enumerate() {
            dados += &format!("<c r=\"{}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>", referencia(j, i), esc(c));
        }
        dados += "</row>";
    }
    let cab = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>";
    let arquivos = [
        ("[Content_Types].xml", format!("{cab}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/></Types>")),
        ("_rels/.rels", format!("{cab}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>")),
        ("xl/workbook.xml", format!("{cab}<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets><sheet name=\"{}\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>", esc(nome))),
        ("xl/_rels/workbook.xml.rels", format!("{cab}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/></Relationships>")),
        ("xl/worksheets/sheet1.xml", format!("{cab}<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>{dados}</sheetData></worksheet>")),
    ];
    let mut z = zip::ZipWriter::new(std::fs::File::create(caminho)?);
    let op = zip::write::SimpleFileOptions::default();
    for (n, c) in arquivos {
        z.start_file(n, op)?;
        z.write_all(c.as_bytes())?;
    }
    z.finish()?;
    Ok(())
}
