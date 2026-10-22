//! Script de build de `orion-setup`: gera `orion-belt.ico` (a constelação com 3 estrelas e blur radial)
//! e embute o ícone nos recursos PE do executável Windows via `rc.exe`.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let ico_path = out_dir.join("orion-belt.ico");
    let rc_path = out_dir.join("orion-setup.rc");
    let res_path = out_dir.join("orion-setup.res");

    let ico_bytes = generate_ico_file(64);
    if fs::write(&ico_path, ico_bytes).is_err() {
        return;
    }

    let escaped_ico = ico_path.display().to_string().replace('\\', "\\\\");
    let rc_content = format!("1 ICON \"{escaped_ico}\"\n");
    if fs::write(&rc_path, rc_content).is_err() {
        return;
    }

    let rc_candidates = [
        r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\rc.exe",
        r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\x64\rc.exe",
        r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.19041.0\x64\rc.exe",
        "rc.exe",
    ];

    for rc_bin in rc_candidates {
        let status = Command::new(rc_bin)
            .args([
                "/nologo",
                "/fo",
                &res_path.display().to_string(),
                &rc_path.display().to_string(),
            ])
            .status();
        if let Ok(st) = status {
            if st.success() && res_path.exists() {
                println!("cargo:rustc-link-arg-bins={}", res_path.display());
                break;
            }
        }
    }
}

fn generate_ico_file(size: u32) -> Vec<u8> {
    let rgba = generate_belt_icon_rgba(size);
    let mut bgra = vec![0u8; rgba.len()];
    // BMP dentro de ICO é bottom-up BGRA
    for y in 0..size {
        let src_y = size - 1 - y;
        for x in 0..size {
            let s_idx = ((src_y * size + x) * 4) as usize;
            let d_idx = ((y * size + x) * 4) as usize;
            bgra[d_idx] = rgba[s_idx + 2];
            bgra[d_idx + 1] = rgba[s_idx + 1];
            bgra[d_idx + 2] = rgba[s_idx];
            bgra[d_idx + 3] = rgba[s_idx + 3];
        }
    }

    let mask_row_bytes = ((size + 31) / 32) * 4;
    let mask_bytes = (mask_row_bytes * size) as usize;
    let dib_size = 40 + bgra.len() + mask_bytes;

    let mut ico = Vec::with_capacity(22 + dib_size);
    // ICONDIR (6 bytes)
    ico.extend_from_slice(&0u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes()); // type = 1 (ICON)
    ico.extend_from_slice(&1u16.to_le_bytes()); // count = 1

    // ICONDIRENTRY (16 bytes)
    ico.push(if size >= 256 { 0 } else { size as u8 });
    ico.push(if size >= 256 { 0 } else { size as u8 });
    ico.push(0);
    ico.push(0);
    ico.extend_from_slice(&1u16.to_le_bytes()); // planes
    ico.extend_from_slice(&32u16.to_le_bytes()); // bpp
    ico.extend_from_slice(&(dib_size as u32).to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes()); // offset

    // BITMAPINFOHEADER (40 bytes)
    ico.extend_from_slice(&40u32.to_le_bytes());
    ico.extend_from_slice(&(size as i32).to_le_bytes());
    ico.extend_from_slice(&((size * 2) as i32).to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&32u16.to_le_bytes());
    ico.extend_from_slice(&0u32.to_le_bytes());
    ico.extend_from_slice(&(bgra.len() as u32).to_le_bytes());
    ico.extend_from_slice(&0i32.to_le_bytes());
    ico.extend_from_slice(&0i32.to_le_bytes());
    ico.extend_from_slice(&0u32.to_le_bytes());
    ico.extend_from_slice(&0u32.to_le_bytes());

    ico.extend_from_slice(&bgra);
    ico.resize(ico.len() + mask_bytes, 0u8);
    ico
}

fn generate_belt_icon_rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let scale = s / 64.0;
    let mut buf = vec![0u8; (size * size * 4) as usize];

    let rx = 14.0 * scale;
    for py in 0..size {
        for px in 0..size {
            let x = px as f32 + 0.5;
            let y = py as f32 + 0.5;
            let dx = if x < rx {
                rx - x
            } else if x > s - rx {
                x - (s - rx)
            } else {
                0.0
            };
            let dy = if y < rx {
                rx - y
            } else if y > s - rx {
                y - (s - rx)
            } else {
                0.0
            };
            let dist = (dx * dx + dy * dy).sqrt();
            let cov = (rx + 0.5 - dist).clamp(0.0, 1.0);
            if cov > 0.0 {
                let idx = ((py * size + px) * 4) as usize;
                buf[idx] = 5;
                buf[idx + 1] = 8;
                buf[idx + 2] = 15;
                buf[idx + 3] = (cov * 255.0).round() as u8;
            }
        }
    }

    let p1 = (16.0 * scale, 40.0 * scale);
    let p2 = (31.0 * scale, 31.0 * scale);
    let p3 = (47.0 * scale, 22.0 * scale);

    // Linha conectando as 3 estrelas (#8fabff)
    let dx = p3.0 - p1.0;
    let dy = p3.1 - p1.1;
    let len_sq = dx * dx + dy * dy;
    let half_w = 0.9 * scale;
    for py in 0..size {
        for px in 0..size {
            let x = px as f32 + 0.5;
            let y = py as f32 + 0.5;
            let t = (((x - p1.0) * dx + (y - p1.1) * dy) / len_sq).clamp(0.0, 1.0);
            let proj_x = p1.0 + t * dx;
            let proj_y = p1.1 + t * dy;
            let dist = ((x - proj_x).powi(2) + (y - proj_y).powi(2)).sqrt();
            let cov = (half_w + 0.7 - dist).clamp(0.0, 1.0);
            if cov > 0.0 {
                let a = cov * 0.55;
                let idx = ((py * size + px) * 4) as usize;
                buf[idx] = (buf[idx] as f32 + 143.0 * a).min(255.0) as u8;
                buf[idx + 1] = (buf[idx + 1] as f32 + 171.0 * a).min(255.0) as u8;
                buf[idx + 2] = (buf[idx + 2] as f32 + 255.0 * a).min(255.0) as u8;
            }
        }
    }

    // Blur radial e núcleo das 3 estrelas
    for &(cx, cy, r_core, r_halo) in &[
        (p1.0, p1.1, 3.6 * scale, 20.0 * scale),
        (p2.0, p2.1, 4.2 * scale, 24.0 * scale),
        (p3.0, p3.1, 3.6 * scale, 20.0 * scale),
    ] {
        for py in 0..size {
            for px in 0..size {
                let x = px as f32 + 0.5;
                let y = py as f32 + 0.5;
                let dist = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
                let idx = ((py * size + px) * 4) as usize;
                if buf[idx + 3] == 0 {
                    continue;
                }
                if dist < r_halo {
                    let norm = (1.0 - dist / r_halo).clamp(0.0, 1.0);
                    let glow = norm * norm * 0.78;
                    buf[idx] = (buf[idx] as f32 + 164.0 * glow).min(255.0) as u8;
                    buf[idx + 1] = (buf[idx + 1] as f32 + 185.0 * glow).min(255.0) as u8;
                    buf[idx + 2] = (buf[idx + 2] as f32 + 255.0 * glow).min(255.0) as u8;
                }
                let core_cov = (r_core + 0.6 - dist).clamp(0.0, 1.0);
                if core_cov > 0.0 {
                    let inv = 1.0 - core_cov;
                    buf[idx] = (buf[idx] as f32 * inv + 238.0 * core_cov) as u8;
                    buf[idx + 1] = (buf[idx + 1] as f32 * inv + 242.0 * core_cov) as u8;
                    buf[idx + 2] = (buf[idx + 2] as f32 * inv + 255.0 * core_cov) as u8;
                }
            }
        }
    }

    buf
}
