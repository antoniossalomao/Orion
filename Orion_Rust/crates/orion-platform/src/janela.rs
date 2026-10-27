//! Janela principal no Windows (F01.25, F01.05): cor de fundo, borda, cantos e Mica pelo DWM.
//!
//! Sem dependências extras: chamadas diretas às DLLs do sistema, como o resto do crate. Tudo aqui é
//! estética e foco; nada lê ou envia dados. Efeitos que a versão do Windows não conhece falham em
//! silêncio (o `HRESULT` volta e o resultado diz o que pegou).

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

#[link(name = "user32")]
extern "system" {
    fn EnumWindows(callback: unsafe extern "system" fn(isize, isize) -> i32, lparam: isize) -> i32;
    fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
    fn IsWindowVisible(hwnd: isize) -> i32;
    fn GetWindow(hwnd: isize, cmd: u32) -> isize;
    fn SetClassLongPtrW(hwnd: isize, index: i32, value: isize) -> usize;
    fn AllowSetForegroundWindow(pid: u32) -> i32;
}
#[link(name = "gdi32")]
extern "system" {
    fn CreateSolidBrush(color: u32) -> isize;
}
#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcessId() -> u32;
}
#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(hwnd: isize, attribute: u32, value: *const c_void, size: u32) -> i32;
    fn DwmExtendFrameIntoClientArea(hwnd: isize, margins: *const Margins) -> i32;
}

#[repr(C)]
struct Margins {
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
}

const GW_OWNER: u32 = 4;
const GCLP_HBRBACKGROUND: i32 = -10;
const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWA_BORDER_COLOR: u32 = 34;
const DWMWA_CAPTION_COLOR: u32 = 35;
const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38;
const DWMWCP_ROUND: u32 = 2;
const DWMSBT_NONE: u32 = 1;
const DWMSBT_MAINWINDOW: u32 = 2;
const ASFW_ANY: u32 = 0xFFFF_FFFF;

struct Busca {
    pid: u32,
    visivel: Option<isize>,
    qualquer: Option<isize>,
}

unsafe extern "system" fn ao_achar(hwnd: isize, lparam: isize) -> i32 {
    let busca = &mut *(lparam as *mut Busca);
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, &mut pid);
    // Só janelas de topo (sem dono) deste processo.
    if pid == busca.pid && GetWindow(hwnd, GW_OWNER) == 0 {
        busca.qualquer.get_or_insert(hwnd);
        if IsWindowVisible(hwnd) != 0 {
            busca.visivel = Some(hwnd);
            return 0;
        }
    }
    1
}

/// A janela de topo deste processo (prefere a visível). Serve de qualquer thread.
pub fn main_window() -> Option<isize> {
    let mut busca = Busca { pid: unsafe { GetCurrentProcessId() }, visivel: None, qualquer: None };
    unsafe { EnumWindows(ao_achar, &mut busca as *mut Busca as isize) };
    busca.visivel.or(busca.qualquer)
}

fn atributo(hwnd: isize, qual: u32, valor: u32) -> bool {
    unsafe { DwmSetWindowAttribute(hwnd, qual, &valor as *const u32 as *const c_void, 4) >= 0 }
}

/// O que pegou.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Estilo {
    /// A janela foi encontrada e o fundo/borda foram aplicados.
    pub aplicado: bool,
    /// O Mica está ligado (Windows 11 22H2 ou mais novo).
    pub mica: bool,
}

/// COLORREF é 0x00BBGGRR.
fn colorref((r, g, b): (u8, u8, u8)) -> u32 {
    (b as u32) << 16 | (g as u32) << 8 | r as u32
}

/// Pinta de grafite tudo o que o Orion não desenha e deixa o Windows 11 consistente com o tema.
///
/// A faixa branca (F01.25/R1.09): sob o renderizador de CPU (WARP) ou com escala fracionária, a
/// superfície do app pode ficar 1 px menor que a janela; o que sobra mostra o pincel de fundo da
/// classe da janela (branco por padrão) e a borda do DWM. Aqui o pincel, a borda e a barra de título
/// viram a cor do tema. Precisa de confirmação na máquina real.
pub fn style_main_window(fundo: (u8, u8, u8), mica: bool) -> Estilo {
    let Some(hwnd) = main_window() else { return Estilo::default() };
    let cor = colorref(fundo);
    unsafe {
        let pincel = CreateSolidBrush(cor);
        if pincel != 0 {
            SetClassLongPtrW(hwnd, GCLP_HBRBACKGROUND, pincel);
        }
    }
    atributo(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, 1);
    atributo(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND);
    atributo(hwnd, DWMWA_BORDER_COLOR, cor);
    atributo(hwnd, DWMWA_CAPTION_COLOR, cor);
    Estilo { aplicado: true, mica: set_mica_em(hwnd, mica) }
}

/// Liga ou desliga o Mica. `true` só se ficou ligado.
pub fn set_mica(ligado: bool) -> bool {
    main_window().is_some_and(|h| set_mica_em(h, ligado))
}

static ESTENDIDO: AtomicBool = AtomicBool::new(false);

fn set_mica_em(hwnd: isize, ligado: bool) -> bool {
    if ligado {
        // O Mica aparece onde a janela é transparente: estende o quadro para dentro da área do app.
        let margens = Margins { left: -1, right: -1, top: -1, bottom: -1 };
        let estendeu = unsafe { DwmExtendFrameIntoClientArea(hwnd, &margens) } >= 0;
        ESTENDIDO.store(estendeu, Ordering::Relaxed);
        estendeu && atributo(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_MAINWINDOW)
    } else {
        if ESTENDIDO.swap(false, Ordering::Relaxed) {
            let margens = Margins { left: 0, right: 0, top: 0, bottom: 0 };
            unsafe { DwmExtendFrameIntoClientArea(hwnd, &margens) };
        }
        atributo(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_NONE);
        false
    }
}

/// Chamada por quem vai pedir a outra cópia do Orion que mostre a janela: sem isso o Windows só
/// pisca o ícone na barra de tarefas em vez de trazer a janela para a frente.
pub fn allow_foreground() {
    unsafe { AllowSetForegroundWindow(ASFW_ANY) };
}
