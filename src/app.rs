use std::{collections::HashMap, fs::File, path::PathBuf};

use anyhow::Result;
use image::{ImageBuffer, ImageReader, Rgba};
use rand::{RngCore, rng};
use rodio::{Decoder, OutputStream, OutputStreamBuilder, Sink};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use wayland_client::{Connection, QueueHandle, protocol::wl_output::WlOutput};

pub struct App {
    output_state: OutputState,
    layer_shell: LayerShell,
    shm: Shm,
    compositor_state: CompositorState,
    registry_state: RegistryState,
    pool: SlotPool,
    layer_surfaces: HashMap<WlOutput, LayerSurface>,
    shown: bool,
    width: u32,
    height: u32,
    image_path: Option<PathBuf>,
    audio_path: Option<PathBuf>,
    output_stream: OutputStream,
    sink: Sink,
}

impl App {
    pub fn new(
        output_state: OutputState,
        layer_shell: LayerShell,
        shm: Shm,
        compositor_state: CompositorState,
        registry_state: RegistryState,
    ) -> Result<Self> {
        let pool = SlotPool::new(1920 * 1080 * 4, &shm)?; // we'll resize this later
        let output_stream =
            OutputStreamBuilder::open_default_stream().expect("open default audio stream");
        let sink = rodio::Sink::connect_new(&output_stream.mixer());

        Ok(Self {
            output_state,
            layer_shell,
            shm,
            compositor_state,
            registry_state,
            pool,
            layer_surfaces: HashMap::new(),
            shown: false,
            width: 0,
            height: 0,
            image_path: None,
            audio_path: None,
            output_stream,
            sink,
        })
    }

    pub fn toggle_overlay(&mut self) {
        for layer in self.layer_surfaces.values() {
            let surface = layer.wl_surface();

            if self.shown {
                self.sink.stop();
                surface.attach(None, 0, 0);
                surface.commit();
            } else {
                self.image_path = None;
                self.audio_path = None;
                layer.set_size(0, 0);
                layer.commit();
            }
        }

        self.shown = !self.shown;
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, output: WlOutput) {
        let surface = self.compositor_state.create_surface(qh);
        let layer_surface = self.layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Top,
            Some("rbar"),
            Some(&output),
        );

        layer_surface.set_anchor(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT | Anchor::BOTTOM);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer_surface.set_size(0, 0);
        layer_surface.set_exclusive_zone(-1);
        layer_surface.commit();
        self.layer_surfaces.insert(output, layer_surface);
    }

    fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {}

    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {
    }
}

impl LayerShellHandler for App {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {}

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        if self.image_path.is_none() {
            self.image_path = Some(random_image());
        }

        if self.audio_path.is_none() {
            self.audio_path = Some(random_audio());
        }

        let (width, height) = configure.new_size;
        self.width = width;
        self.height = height;

        let stride = width * 4;
        let size = stride * height;
        self.pool.resize(size as usize).unwrap();
        let (buffer, canvas) = self
            .pool
            .create_buffer(
                width as i32,
                height as i32,
                stride as i32,
                wayland_client::protocol::wl_shm::Format::Argb8888,
            )
            .expect("slotpool create_buffer failed");
        let surface = layer.wl_surface();
        surface.attach(Some(&buffer.wl_buffer()), 0, 0);
        surface.damage_buffer(0, 0, width as i32, height as i32);
        surface.commit();

        let img = ImageReader::open(self.image_path.as_ref().unwrap())
            .unwrap()
            .decode()
            .unwrap()
            .to_rgba8();
        draw(canvas, self.width, self.height, img);

        let file = File::open(self.audio_path.as_ref().unwrap()).unwrap();
        let source = Decoder::try_from(file).unwrap();
        self.sink.append(source);
        self.sink.play();
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut smithay_client_toolkit::registry::RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState];
}

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wayland_client::protocol::wl_surface::WlSurface,
        _new_factor: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wayland_client::protocol::wl_surface::WlSurface,
        _new_transform: wayland_client::protocol::wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wayland_client::protocol::wl_surface::WlSurface,
        _time: u32,
    ) {
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wayland_client::protocol::wl_surface::WlSurface,
        _output: &wayland_client::protocol::wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wayland_client::protocol::wl_surface::WlSurface,
        _output: &wayland_client::protocol::wl_output::WlOutput,
    ) {
    }
}

smithay_client_toolkit::delegate_output!(App);
smithay_client_toolkit::delegate_layer!(App);
smithay_client_toolkit::delegate_registry!(App);
smithay_client_toolkit::delegate_shm!(App);
smithay_client_toolkit::delegate_compositor!(App);

fn draw(canvas: &mut [u8], width: u32, height: u32, image: ImageBuffer<Rgba<u8>, Vec<u8>>) {
    let img_width = image.width() as usize;
    let img_height = image.height() as usize;
    let img_pixels = image.into_raw();

    for px in canvas.chunks_exact_mut(4) {
        px[0] = 128;
        px[1] = 128;
        px[2] = 128;
        px[3] = 196;
    }

    let offset_x = ((width as usize) - img_width) / 2;
    let offset_y = ((height as usize) - img_height) / 2;

    for y in 0..img_height {
        for x in 0..img_width {
            let src_i = (y * img_width + x) * 4;

            let dst_x = offset_x + x;
            let dst_y = offset_y + y;

            if dst_x >= width as usize || dst_y >= height as usize {
                continue;
            }

            let dst_i = (dst_y * width as usize + dst_x) * 4;

            let sr = img_pixels[src_i + 0] as f32;
            let sg = img_pixels[src_i + 1] as f32;
            let sb = img_pixels[src_i + 2] as f32;
            let sa = img_pixels[src_i + 3] as f32 / 255.0;

            if sa == 0.0 {
                continue;
            }

            let dr = canvas[dst_i + 2] as f32;
            let dg = canvas[dst_i + 1] as f32;
            let db = canvas[dst_i + 0] as f32;
            let da = canvas[dst_i + 3] as f32 / 255.0;

            let out_a = sa + da * (1.0 - sa);
            let out_r = (sr * sa + dr * da * (1.0 - sa)) / out_a;
            let out_g = (sg * sa + dg * da * (1.0 - sa)) / out_a;
            let out_b = (sb * sa + db * da * (1.0 - sa)) / out_a;

            canvas[dst_i + 2] = out_r as u8;
            canvas[dst_i + 1] = out_g as u8;
            canvas[dst_i + 0] = out_b as u8;
            canvas[dst_i + 3] = (out_a * 255.0) as u8;
        }
    }
}

fn random_image() -> PathBuf {
    let mut rng = rng();
    let file_paths: Vec<PathBuf> = std::fs::read_dir("images")
        .unwrap()
        .map(|e| e.unwrap())
        .map(|e| e.path())
        .collect();

    let i = rng.next_u32() as usize % file_paths.len();
    file_paths[i].clone()
}

fn random_audio() -> PathBuf {
    let mut rng = rng();
    let file_paths: Vec<PathBuf> = std::fs::read_dir("music")
        .unwrap()
        .map(|e| e.unwrap())
        .map(|e| e.path())
        .collect();

    let i = rng.next_u32() as usize % file_paths.len();
    file_paths[i].clone()
}
