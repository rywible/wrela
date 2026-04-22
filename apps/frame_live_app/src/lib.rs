use eframe::egui::{self, Color32, ColorImage, Pos2, Rect, RichText, ScrollArea, Sense, Vec2};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Duration;
use wrela::frame_live::{
    FRAME_LIVE_RELOAD_POLL_MS, FrameLiveFrame, FrameLiveLaunchConfig, FrameLiveSession, FramePixel,
    SelectionRecord, render_selection_record_human,
};

#[derive(Debug, Clone, PartialEq)]
pub enum WorkerCommand {
    ForceReload,
    Select(FramePixel),
    Shutdown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorkerEvent {
    FrameReady(FrameLiveFrame),
    SelectionReady(SelectionRecord),
    ReloadFailed(String),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FrameLiveAppModel {
    pub current_frame: Option<FrameLiveFrame>,
    pub current_selection: Option<SelectionRecord>,
    pub selection_history: Vec<SelectionRecord>,
    pub status_text: String,
    pub reload_error: Option<String>,
}

impl FrameLiveAppModel {
    pub fn apply_event(&mut self, event: WorkerEvent) {
        match event {
            WorkerEvent::FrameReady(frame) => {
                self.status_text = format!(
                    "generation {} | {}x{} | view={} | region={}",
                    frame.generation, frame.width, frame.height, frame.view_name, frame.region_name
                );
                self.reload_error = None;
                self.current_frame = Some(frame);
            }
            WorkerEvent::SelectionReady(selection) => {
                self.status_text = format!(
                    "selected frame pixel ({}, {}) on generation {}",
                    selection.frame_pixel.x, selection.frame_pixel.y, selection.generation
                );
                self.current_selection = Some(selection.clone());
                self.selection_history.push(selection);
            }
            WorkerEvent::ReloadFailed(error) => {
                self.status_text = "reload failed; keeping last good frame".to_string();
                self.reload_error = Some(error);
            }
        }
    }

    pub fn clear_history(&mut self) {
        self.selection_history.clear();
    }
}

pub fn displayed_image_rect(available_rect: Rect, frame_size: (u32, u32)) -> Rect {
    let (frame_width, frame_height) = frame_size;
    if frame_width == 0
        || frame_height == 0
        || available_rect.width() <= 0.0
        || available_rect.height() <= 0.0
    {
        return Rect::from_min_size(available_rect.min, Vec2::ZERO);
    }
    let scale = (available_rect.width() / frame_width as f32)
        .min(available_rect.height() / frame_height as f32);
    let size = Vec2::new(frame_width as f32 * scale, frame_height as f32 * scale);
    Rect::from_center_size(available_rect.center(), size)
}

pub fn map_pointer_to_frame_pixel(
    pointer: Pos2,
    image_rect: Rect,
    frame_size: (u32, u32),
) -> FramePixel {
    let (frame_width, frame_height) = frame_size;
    if frame_width <= 1
        || frame_height <= 1
        || image_rect.width() <= 0.0
        || image_rect.height() <= 0.0
    {
        return FramePixel { x: 0, y: 0 };
    }
    let normalized_x = ((pointer.x - image_rect.min.x) / image_rect.width()).clamp(0.0, 1.0);
    let normalized_y = ((pointer.y - image_rect.min.y) / image_rect.height()).clamp(0.0, 1.0);
    let x = (normalized_x * frame_width as f32).floor() as u32;
    let y = (normalized_y * frame_height as f32).floor() as u32;
    FramePixel {
        x: x.min(frame_width.saturating_sub(1)),
        y: y.min(frame_height.saturating_sub(1)),
    }
}

pub fn run_from_args(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let launch_config_path = parse_launch_config_arg(args)?;
    let launch_config = load_launch_config(&launch_config_path)?;
    run_app(launch_config).map_err(|err| err.to_string())
}

fn parse_launch_config_arg(args: impl IntoIterator<Item = String>) -> Result<PathBuf, String> {
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--launch-config" {
            let Some(path) = args.next() else {
                return Err("missing value after --launch-config".to_string());
            };
            return Ok(PathBuf::from(path));
        }
    }
    Err("missing required --launch-config <path>".to_string())
}

fn load_launch_config(path: &Path) -> Result<FrameLiveLaunchConfig, String> {
    let body = fs::read_to_string(path)
        .map_err(|err| format!("read launch config `{}`: {err}", path.display()))?;
    serde_json::from_str(&body)
        .map_err(|err| format!("decode launch config `{}`: {err}", path.display()))
}

fn run_app(config: FrameLiveLaunchConfig) -> Result<(), eframe::Error> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1440.0, 900.0]),
        ..eframe::NativeOptions::default()
    };
    eframe::run_native(
        "Wrela Frame Live",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(FrameLiveApp::new(
                config.clone(),
                cc.egui_ctx.clone(),
            )))
        }),
    )
}

fn spawn_worker(
    config: FrameLiveLaunchConfig,
    repaint_ctx: egui::Context,
) -> (Sender<WorkerCommand>, Receiver<WorkerEvent>, JoinHandle<()>) {
    let (command_tx, command_rx) = mpsc::channel::<WorkerCommand>();
    let (event_tx, event_rx) = mpsc::channel::<WorkerEvent>();
    let handle = std::thread::spawn(move || {
        let mut session = match FrameLiveSession::load(config.clone()) {
            Ok(session) => {
                send_worker_event(
                    &event_tx,
                    &repaint_ctx,
                    WorkerEvent::FrameReady(session.frame()),
                );
                Some(session)
            }
            Err(err) => {
                send_worker_event(
                    &event_tx,
                    &repaint_ctx,
                    WorkerEvent::ReloadFailed(err.render_human()),
                );
                None
            }
        };
        loop {
            match command_rx.recv_timeout(Duration::from_millis(FRAME_LIVE_RELOAD_POLL_MS)) {
                Ok(WorkerCommand::ForceReload) => {
                    if let Some(active_session) = session.as_mut() {
                        match active_session.force_reload() {
                            Ok(frame) => {
                                send_worker_event(
                                    &event_tx,
                                    &repaint_ctx,
                                    WorkerEvent::FrameReady(frame),
                                );
                            }
                            Err(err) => {
                                send_worker_event(
                                    &event_tx,
                                    &repaint_ctx,
                                    WorkerEvent::ReloadFailed(err.render_human()),
                                );
                            }
                        }
                    } else {
                        session = match FrameLiveSession::load(config.clone()) {
                            Ok(new_session) => {
                                send_worker_event(
                                    &event_tx,
                                    &repaint_ctx,
                                    WorkerEvent::FrameReady(new_session.frame()),
                                );
                                Some(new_session)
                            }
                            Err(err) => {
                                send_worker_event(
                                    &event_tx,
                                    &repaint_ctx,
                                    WorkerEvent::ReloadFailed(err.render_human()),
                                );
                                None
                            }
                        };
                    }
                }
                Ok(WorkerCommand::Select(frame_pixel)) => {
                    if let Some(active_session) = session.as_ref()
                        && let Ok(selection) =
                            active_session.selection_record(frame_pixel, frame_pixel)
                    {
                        send_worker_event(
                            &event_tx,
                            &repaint_ctx,
                            WorkerEvent::SelectionReady(selection),
                        );
                    }
                }
                Ok(WorkerCommand::Shutdown) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            if let Some(active_session) = session.as_mut() {
                match active_session.reload_if_sources_changed() {
                    Ok(Some(frame)) => {
                        send_worker_event(&event_tx, &repaint_ctx, WorkerEvent::FrameReady(frame));
                    }
                    Ok(None) => {}
                    Err(err) => {
                        send_worker_event(
                            &event_tx,
                            &repaint_ctx,
                            WorkerEvent::ReloadFailed(err.render_human()),
                        );
                    }
                }
            }
        }
    });
    (command_tx, event_rx, handle)
}

fn send_worker_event(
    event_tx: &Sender<WorkerEvent>,
    repaint_ctx: &egui::Context,
    event: WorkerEvent,
) {
    let _ = event_tx.send(event);
    repaint_ctx.request_repaint();
}

struct FrameLiveApp {
    model: FrameLiveAppModel,
    command_tx: Sender<WorkerCommand>,
    event_rx: Receiver<WorkerEvent>,
    worker_handle: Option<JoinHandle<()>>,
    texture: Option<egui::TextureHandle>,
    texture_generation: Option<u64>,
}

impl FrameLiveApp {
    fn new(config: FrameLiveLaunchConfig, repaint_ctx: egui::Context) -> Self {
        let (command_tx, event_rx, worker_handle) = spawn_worker(config, repaint_ctx);
        Self {
            model: FrameLiveAppModel::default(),
            command_tx,
            event_rx,
            worker_handle: Some(worker_handle),
            texture: None,
            texture_generation: None,
        }
    }

    fn pump_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            self.model.apply_event(event);
        }
    }

    fn ensure_texture(&mut self, ctx: &egui::Context) {
        let Some(frame) = self.model.current_frame.as_ref() else {
            return;
        };
        if self.texture_generation == Some(frame.generation) {
            return;
        }
        let image = frame_to_color_image(frame);
        match self.texture.as_mut() {
            Some(texture) => texture.set(image, egui::TextureOptions::NEAREST),
            None => {
                self.texture = Some(ctx.load_texture(
                    "frame-live-color",
                    image,
                    egui::TextureOptions::NEAREST,
                ));
            }
        }
        self.texture_generation = Some(frame.generation);
    }
}

impl Drop for FrameLiveApp {
    fn drop(&mut self) {
        let _ = self.command_tx.send(WorkerCommand::Shutdown);
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }
}

impl eframe::App for FrameLiveApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump_events();
        self.ensure_texture(ctx);

        egui::TopBottomPanel::top("frame-live-controls").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Reload").clicked() {
                    let _ = self.command_tx.send(WorkerCommand::ForceReload);
                }
                if ui.button("Clear History").clicked() {
                    self.model.clear_history();
                }
                if !self.model.status_text.is_empty() {
                    ui.label(RichText::new(&self.model.status_text).monospace());
                }
            });
        });

        egui::SidePanel::right("frame-live-sidebar")
            .min_width(380.0)
            .show(ctx, |ui| {
                ui.heading("Selection");
                if let Some(error) = &self.model.reload_error {
                    ui.colored_label(Color32::LIGHT_RED, error);
                    ui.separator();
                }
                ui.label("Current");
                if let Some(selection) = &self.model.current_selection {
                    ui.code(render_selection_record_human(selection));
                } else {
                    ui.label("No selection yet.");
                }
                ui.separator();
                ui.label(format!("History ({})", self.model.selection_history.len()));
                ScrollArea::vertical().show(ui, |ui| {
                    for selection in self.model.selection_history.iter().rev() {
                        ui.code(render_selection_record_human(selection));
                        ui.separator();
                    }
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let available_rect = ui.max_rect();
            ui.painter()
                .rect_filled(available_rect, 0.0, Color32::from_gray(18));

            let Some(frame) = self.model.current_frame.as_ref() else {
                ui.centered_and_justified(|ui| {
                    if let Some(error) = &self.model.reload_error {
                        ui.colored_label(Color32::LIGHT_RED, error);
                    } else {
                        ui.label("Waiting for frame...");
                    }
                });
                return;
            };
            let image_rect = displayed_image_rect(available_rect, (frame.width, frame.height));
            let Some(texture) = self.texture.as_ref() else {
                return;
            };
            let image = egui::Image::new(texture)
                .fit_to_exact_size(image_rect.size())
                .sense(Sense::click());
            let response = ui.put(image_rect, image);
            if response.clicked()
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let frame_pixel =
                    map_pointer_to_frame_pixel(pointer, image_rect, (frame.width, frame.height));
                let _ = self.command_tx.send(WorkerCommand::Select(frame_pixel));
            }
        });
    }
}

fn frame_to_color_image(frame: &FrameLiveFrame) -> ColorImage {
    let pixels: Vec<Color32> = frame
        .color_buffer
        .iter()
        .map(|value| {
            let r = ((value >> 16) & 0xff) as u8;
            let g = ((value >> 8) & 0xff) as u8;
            let b = (value & 0xff) as u8;
            Color32::from_rgb(r, g, b)
        })
        .collect();
    ColorImage {
        size: [frame.width as usize, frame.height as usize],
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_frame(generation: u64) -> FrameLiveFrame {
        FrameLiveFrame {
            generation,
            width: 4,
            height: 2,
            color_buffer: vec![0; 8],
            view_name: "main_view".to_string(),
            region_name: "scene_region".to_string(),
        }
    }

    fn sample_selection(x: u32) -> SelectionRecord {
        SelectionRecord {
            generation: 1,
            window_pixel: FramePixel { x, y: 0 },
            frame_pixel: FramePixel { x, y: 0 },
            hit: true,
            region_name: Some("scene_region".to_string()),
            shape_name: Some("shape".to_string()),
            field_name: Some("field".to_string()),
            root_shape_id: Some(1),
            feature_id: Some(2),
            instance_id: Some(3),
            repeat_id: Some(4),
            world_position: Some([1.0, 2.0, 3.0]),
            normal: Some([0.0, 1.0, 0.0]),
            primary_source: None,
            source_refs: Vec::new(),
        }
    }

    #[test]
    fn selection_history_appends_and_clears() {
        let mut model = FrameLiveAppModel::default();
        model.apply_event(WorkerEvent::SelectionReady(sample_selection(1)));
        model.apply_event(WorkerEvent::SelectionReady(sample_selection(2)));
        assert_eq!(model.selection_history.len(), 2);
        model.clear_history();
        assert!(model.selection_history.is_empty());
        assert!(model.current_selection.is_some());
    }

    #[test]
    fn reload_failure_preserves_last_good_frame_model() {
        let mut model = FrameLiveAppModel::default();
        let frame = sample_frame(7);
        model.apply_event(WorkerEvent::FrameReady(frame.clone()));
        model.apply_event(WorkerEvent::ReloadFailed("boom".to_string()));
        assert_eq!(model.current_frame, Some(frame));
        assert_eq!(model.reload_error.as_deref(), Some("boom"));
    }

    #[test]
    fn current_selection_updates_without_losing_prior_history() {
        let mut model = FrameLiveAppModel::default();
        let first = sample_selection(1);
        let second = sample_selection(2);
        model.apply_event(WorkerEvent::SelectionReady(first.clone()));
        model.apply_event(WorkerEvent::SelectionReady(second.clone()));
        assert_eq!(model.current_selection, Some(second));
        assert_eq!(model.selection_history, vec![first, sample_selection(2)]);
    }

    #[test]
    fn viewport_mapping_exact_fit_uses_full_frame() {
        let rect = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(640.0, 360.0));
        assert_eq!(
            map_pointer_to_frame_pixel(Pos2::new(639.0, 359.0), rect, (640, 360)),
            FramePixel { x: 639, y: 359 }
        );
    }

    #[test]
    fn viewport_mapping_scaled_up_preserves_pixel_coordinates() {
        let rect = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(1280.0, 720.0));
        assert_eq!(
            map_pointer_to_frame_pixel(Pos2::new(640.0, 360.0), rect, (640, 360)),
            FramePixel { x: 320, y: 180 }
        );
    }

    #[test]
    fn viewport_mapping_letterboxed_uses_image_rect_not_window_rect() {
        let available = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(1200.0, 900.0));
        let image_rect = displayed_image_rect(available, (640, 360));
        assert!(image_rect.min.y > available.min.y);
        assert_eq!(
            map_pointer_to_frame_pixel(image_rect.center(), image_rect, (640, 360)),
            FramePixel { x: 320, y: 180 }
        );
    }

    #[test]
    fn viewport_mapping_clamps_edges_and_corners() {
        let rect = Rect::from_min_size(Pos2::new(100.0, 50.0), Vec2::new(640.0, 360.0));
        assert_eq!(
            map_pointer_to_frame_pixel(Pos2::new(0.0, 0.0), rect, (640, 360)),
            FramePixel { x: 0, y: 0 }
        );
        assert_eq!(
            map_pointer_to_frame_pixel(Pos2::new(1000.0, 1000.0), rect, (640, 360)),
            FramePixel { x: 639, y: 359 }
        );
    }
}
