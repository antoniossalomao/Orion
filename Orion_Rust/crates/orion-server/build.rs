//! Script de build do `orion.exe`. M13: embute o ícone oficial do Orion (`assets/orion.ico`) e B15: a
//! VERSIONINFO (produto, versão e descrição) nos recursos PE, pelo `rc.exe` do Windows SDK. Fora do
//! Windows, ou sem `rc.exe`, não faz nada: o executável só fica sem ícone, como antes.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let ico = manifest_dir.join("../../../assets/orion.ico");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", ico.display());
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") || !ico.exists() {
        return;
    }
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let rc_path = out_dir.join("orion.rc");
    let res_path = out_dir.join("orion.res");
    let ico = ico.canonicalize().unwrap_or(ico);
    let escaped_ico = ico.display().to_string().trim_start_matches(r"\\?\").replace('\\', "\\\\");
    let v = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let n: Vec<u16> = v.split('.').map(|p| p.parse().unwrap_or(0)).chain(std::iter::repeat(0)).take(4).collect();
    let comma = format!("{},{},{},{}", n[0], n[1], n[2], n[3]);
    let rc = format!(
        "1 ICON \"{escaped_ico}\"\n1 VERSIONINFO\nFILEVERSION {comma}\nPRODUCTVERSION {comma}\nFILEOS 0x40004\nFILETYPE 0x1\nBEGIN\n BLOCK \"StringFileInfo\"\n BEGIN\n  BLOCK \"041604B0\"\n  BEGIN\n   VALUE \"CompanyName\", \"Orion\"\n   VALUE \"FileDescription\", \"Orion\"\n   VALUE \"FileVersion\", \"{v}\"\n   VALUE \"InternalName\", \"orion.exe\"\n   VALUE \"OriginalFilename\", \"orion.exe\"\n   VALUE \"ProductName\", \"Orion\"\n   VALUE \"ProductVersion\", \"{v}\"\n  END\n END\n BLOCK \"VarFileInfo\"\n BEGIN\n  VALUE \"Translation\", 0x0416, 1200\n END\nEND\n"
    );
    if fs::write(&rc_path, rc).is_err() {
        return;
    }
    let rc_candidates = [
        r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\rc.exe",
        r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\x64\rc.exe",
        r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.19041.0\x64\rc.exe",
        "rc.exe",
    ];
    for rc_bin in rc_candidates {
        let status = Command::new(rc_bin).args(["/nologo", "/fo", &res_path.display().to_string(), &rc_path.display().to_string()]).status();
        if status.is_ok_and(|st| st.success()) && res_path.exists() {
            println!("cargo:rustc-link-arg-bins={}", res_path.display());
            break;
        }
    }
}
