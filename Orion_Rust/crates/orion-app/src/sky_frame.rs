//! F01.29: o céu do `orion-sky` também no login e na tela inicial, no mesmo desenho do instalador:
//! o frame é composto numa thread própria, com um único frame pendente (nunca acumula atraso), a resolução cai
//! sozinha se a composição passar de 14 ms, e nada é pedido com a janela sem foco ou minimizada.
//! Com "reduzir movimento" (Orion ou Windows) o céu fica num frame estático.
//!
//! Com GPU o frame não passa pelo widget de imagem: o iced manda toda imagem nova com mais de 2 MB
//! para uma fila de envio em segundo plano, não desenha nada até ela chegar e cria uma textura
//! inteira por imagem. Com um frame novo a cada tick o céu piscava e a memória só subia. Aqui os
//! pixels vão para uma única textura, reescrita no lugar (`SkyPrimitive`).

use iced::advanced::graphics::core::Bytes;
use iced::wgpu;
use iced::widget::image::Handle;
use iced::widget::shader;
use iced::{Element, Length, Rectangle};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::{Duration, Instant};

const FRAME_LIMIT: Duration = Duration::from_millis(14);
const MIN_SCALE: f32 = 0.5;

fn hardware() -> bool {
    std::env::var("ICED_BACKEND").is_ok_and(|b| b == "wgpu")
}

/// Um frame composto do céu. `serial` muda a cada composição: é como a GPU sabe que precisa reenviar.
#[derive(Clone, Debug)]
pub struct Frame {
    size: (u32, u32),
    pixels: Bytes,
    serial: u64,
    /// Só no renderizador por CPU, que desenha pelo widget de imagem.
    handle: Option<Handle>,
}

impl Frame {
    pub fn compose(sky: &orion_sky::OrionSky) -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(1);
        let (w, h, pixels) = sky.render_frame();
        let pixels = Bytes::from(pixels);
        let handle = (!hardware()).then(|| Handle::from_rgba(w, h, pixels.clone()));
        Self { size: (w, h), pixels, serial: SERIAL.fetch_add(1, Ordering::Relaxed), handle }
    }

    /// O frame esticado na área toda. `slot` separa as texturas de quem desenha céus diferentes.
    pub fn view<'a, M: 'a>(&self, slot: Slot) -> Element<'a, M> {
        match &self.handle {
            Some(handle) => iced::widget::image(handle.clone()).content_fit(iced::ContentFit::Fill).width(Length::Fill).height(Length::Fill).into(),
            None => shader(SkyProgram { frame: self.clone(), slot }).width(Length::Fill).height(Length::Fill).into(),
        }
    }
}

/// Quem desenha o céu: cada um tem a própria textura na GPU.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    /// Login e tela inicial (o mesmo `SkyFrame`).
    App,
    Onboarding,
}

struct SkyProgram {
    frame: Frame,
    slot: Slot,
}

impl<M> shader::Program<M> for SkyProgram {
    type State = ();
    type Primitive = SkyPrimitive;

    fn draw(&self, _state: &(), _cursor: iced::mouse::Cursor, _bounds: Rectangle) -> SkyPrimitive {
        SkyPrimitive { frame: self.frame.clone(), slot: self.slot }
    }
}

#[derive(Debug)]
pub struct SkyPrimitive {
    frame: Frame,
    slot: Slot,
}

struct Texture {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    size: (u32, u32),
    serial: u64,
    used: bool,
}

pub struct SkyPipeline {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    format: wgpu::TextureFormat,
    textures: HashMap<Slot, Texture>,
}

const SHADER: &str = r"
struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}
@vertex
fn vs(@builtin(vertex_index) i: u32) -> Out {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: Out;
    o.pos = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(p.x, 1.0 - p.y);
    return o;
}
@group(0) @binding(0) var sky: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;
@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    return textureSample(sky, smp, in.uv);
}
";

impl shader::Pipeline for SkyPipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("orion sky"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("orion sky"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("orion sky"), bind_group_layouts: &[&layout], push_constant_ranges: &[] });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("orion sky"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
            // O céu é opaco: sem mistura, ele substitui o que está embaixo.
            fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some("fs"), targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor { label: Some("orion sky"), mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });
        Self { pipeline, layout, sampler, format, textures: HashMap::new() }
    }

    /// Fim do quadro: a textura de um céu que não foi desenhado (saiu da tela) é devolvida.
    fn trim(&mut self) {
        self.textures.retain(|_, t| std::mem::take(&mut t.used));
    }
}

impl shader::Primitive for SkyPrimitive {
    type Pipeline = SkyPipeline;

    fn prepare(&self, pipeline: &mut SkyPipeline, device: &wgpu::Device, queue: &wgpu::Queue, _bounds: &Rectangle, _viewport: &shader::Viewport) {
        let (w, h) = self.frame.size;
        if self.frame.pixels.len() != (w as usize) * (h as usize) * 4 || w == 0 || h == 0 {
            return;
        }
        if pipeline.textures.get(&self.slot).is_none_or(|t| t.size != (w, h)) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("orion sky"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                // Mesma regra do atlas de imagens do iced: destino sRGB lê os pixels como sRGB.
                format: if pipeline.format.is_srgb() { wgpu::TextureFormat::Rgba8UnormSrgb } else { wgpu::TextureFormat::Rgba8Unorm },
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("orion sky"),
                layout: &pipeline.layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&pipeline.sampler) }],
            });
            pipeline.textures.insert(self.slot, Texture { texture, bind_group, size: (w, h), serial: 0, used: true });
        }
        let Some(t) = pipeline.textures.get_mut(&self.slot) else { return };
        t.used = true;
        if t.serial != self.frame.serial {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &t.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                &self.frame.pixels,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * w), rows_per_image: Some(h) },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
            t.serial = self.frame.serial;
        }
    }

    fn draw(&self, pipeline: &SkyPipeline, pass: &mut wgpu::RenderPass<'_>) -> bool {
        if let Some(t) = pipeline.textures.get(&self.slot) {
            pass.set_pipeline(&pipeline.pipeline);
            pass.set_bind_group(0, &t.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        true
    }
}

pub struct SkyFrame {
    sky: orion_sky::OrionSky,
    pub frame: Frame,
    request_tx: SyncSender<orion_sky::OrionSky>,
    result_rx: Receiver<(Frame, Duration)>,
    busy: bool,
    last_tick: Option<Instant>,
    size: (u32, u32),
    scale: f32,
    wants_frame: bool,
}

/// Frame do céu: a área onde ele aparece, na densidade da tela, para não ser esticado nem cortado.
/// O maior lado tem teto (REGRAS.md: portabilidade e RAM < 120 MB): 2560 px com GPU, 1920 na CPU;
/// `scale` é a redução automática quando a composição fica lenta.
/// Devolve o tamanho em pixels e quantos pixels cabem em cada ponto lógico.
fn frame_pixels(area: iced::Size, density: f32, scale: f32) -> ((u32, u32), f32) {
    let (w, h) = (area.width.max(64.0), area.height.max(64.0));
    let cap = if hardware() { 2560.0 } else { 1920.0 };
    let k = (density.clamp(0.5, 4.0) * scale).min(cap * scale / w.max(h));
    ((((w * k).round() as u32).max(64), ((h * k).round() as u32).max(64)), k)
}

impl SkyFrame {
    pub fn new(area: iced::Size, density: f32) -> Self {
        let mut sky = orion_sky::OrionSky::new();
        let (size, k) = frame_pixels(area, density, 1.0);
        sky.set_pixel_scale(k);
        sky.set_size(size.0, size.1);
        sky.one_star_per_step();
        // Constelação distante: ocupa o centro da área e deixa o céu em volta.
        sky.compact_constellation(0.6);
        sky.light_to(7);
        for _ in 0..160 {
            sky.step_dt(0.05, 0.0, 0.0);
        }
        let frame = Frame::compose(&sky);
        let (request_tx, requests) = mpsc::sync_channel::<orion_sky::OrionSky>(1);
        let (results, result_rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            while let Ok(sky) = requests.recv() {
                let started = Instant::now();
                let frame = Frame::compose(&sky);
                if results.send((frame, started.elapsed())).is_err() {
                    break;
                }
            }
        });
        Self { sky, frame, request_tx, result_rx, busy: false, last_tick: None, size, scale: 1.0, wants_frame: false }
    }

    /// Janela sem foco: o relógio recomeça sem salto quando ela voltar.
    pub fn pause(&mut self) {
        self.last_tick = None;
    }

    /// Chamado pelo relógio de quadros. `animate` falso mantém um frame estático (só refaz ao mudar o size).
    pub fn tick(&mut self, now: Instant, area: iced::Size, density: f32, animate: bool) {
        let dt = self.last_tick.map(|p| now.saturating_duration_since(p).as_secs_f32()).unwrap_or(0.05).clamp(0.001, 0.05);
        self.last_tick = Some(now);
        if let Ok((frame, spent)) = self.result_rx.try_recv() {
            self.frame = frame;
            self.busy = false;
            if spent > FRAME_LIMIT && self.scale > MIN_SCALE {
                self.scale = (self.scale * 0.85).max(MIN_SCALE);
                self.wants_frame = true;
            }
        }
        self.resize(area, density);
        if animate {
            self.sky.step_dt(dt, 0.0, 0.0);
        }
        if animate {
            self.wants_frame = true;
        }
        self.request();
    }

    fn resize(&mut self, area: iced::Size, density: f32) {
        let (new_size, k) = frame_pixels(area, density, self.scale);
        if new_size != self.size {
            self.size = new_size;
            self.sky.set_pixel_scale(k);
            self.sky.set_size(new_size.0, new_size.1);
            self.wants_frame = true;
        }
    }

    fn request(&mut self) {
        if self.wants_frame && !self.busy && self.request_tx.try_send(self.sky.clone()).is_ok() {
            self.busy = true;
            self.wants_frame = false;
        }
    }

    /// Fora do relógio: pede um frame novo assim que a área ou a densidade mudam
    /// (login → início, barra recolhida, outro monitor), sem avançar a animação.
    pub fn adjust(&mut self, area: iced::Size, density: f32) {
        self.resize(area, density);
        self.request();
    }

    /// Compõe na hora, sem a thread: para as fotos (`snap`), que não têm relógio.
    pub fn settle(&mut self, area: iced::Size, density: f32) {
        self.resize(area, density);
        self.frame = Frame::compose(&self.sky);
        self.wants_frame = false;
    }
}

/// "Mostrar animações" desligado no Windows (`SPI_GETCLIENTAREAANIMATION`); fora do Windows, falso.
#[cfg(windows)]
pub fn system_reduces_motion() -> bool {
    #[link(name = "user32")]
    extern "system" {
        fn SystemParametersInfoW(action: u32, param: u32, pv_param: *mut std::ffi::c_void, win_ini: u32) -> i32;
    }
    let mut enabled: i32 = 1;
    // SAFETY: `enabled` é um BOOL válido durante a chamada; SPI_GETCLIENTAREAANIMATION (0x1042) só escreve nele.
    let ok = unsafe { SystemParametersInfoW(0x1042, 0, &mut enabled as *mut i32 as *mut std::ffi::c_void, 0) };
    ok != 0 && enabled == 0
}

#[cfg(not(windows))]
pub fn system_reduces_motion() -> bool {
    false
}
