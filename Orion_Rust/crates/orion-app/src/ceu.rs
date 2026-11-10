//! F01.29: o céu do `orion-sky` também no login e na tela inicial.
//! Por enquanto um quadro estático; a animação (20/60 fps, pausa fora de vista, reduzir movimento) exige o
//! widget wgpu que reaproveita uma textura (F16.06/F17), sem criar uma imagem nova a cada quadro.

pub struct Ceu {
    sky: orion_sky::OrionSky,
    pub img: iced::widget::image::Handle,
    tamanho: (u32, u32),
}

fn hardware() -> bool {
    std::env::var("ICED_BACKEND").is_ok_and(|b| b == "wgpu")
}

/// Resolução do quadro: a da janela com GPU; na CPU, reduzida (REGRAS.md: portabilidade e RAM < 120 MB).
fn pixels(win: iced::Size) -> (u32, u32) {
    let (w, h) = (win.width.max(200.0), win.height.max(300.0));
    let alvo = if hardware() { 1280.0 } else { 800.0 };
    let k = (alvo / w.max(h)).min(1.0);
    ((w * k) as u32, (h * k) as u32)
}

impl Ceu {
    /// Um único quadro assentado (todas as estrelas acesas). Quadro novo só ao mudar o tamanho da janela:
    /// uma textura nova por quadro de animação enchia a memória da GPU, piscava e derrubava o app.
    pub fn new(win: iced::Size) -> Self {
        let mut sky = orion_sky::OrionSky::new();
        let tamanho = pixels(win);
        sky.set_size(tamanho.0, tamanho.1);
        sky.one_star_per_step();
        sky.compact_constellation(0.8);
        sky.light_to(7);
        for _ in 0..160 {
            sky.step_dt(0.05, 0.0, 0.0);
        }
        let img = sky.render_image_handle();
        Self { sky, img, tamanho }
    }

    pub fn resize(&mut self, win: iced::Size) {
        let novo = pixels(win);
        if novo == self.tamanho {
            return;
        }
        self.tamanho = novo;
        self.sky.set_size(novo.0, novo.1);
        self.img = self.sky.render_image_handle();
    }
}
