// --image-url: url("https://i.redd.it/red-forest-1920-1080-v0-s9u8ki2rr70a1.jpg?s=139edf608c428656505a143635a0687dec086229");
use eframe::{egui, App, Frame};
use std::sync::mpsc;
use std::thread;

pub struct MyApp {
    background: Option<egui::TextureHandle>,
    receiver: mpsc::Receiver<(Vec<u8>, [usize; 2])>,
}

impl MyApp {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let url = "https://i.redd.it/red-forest-1920-1080-v0-s9u8ki2rr70a1.jpg?s=139edf608c428656505a143635a0687dec086229";

        thread::spawn(move || {
            if let Ok(resp) = reqwest::blocking::get(url) {
                if let Ok(bytes) = resp.bytes() {
                    if let Ok(image) = image::load_from_memory(&bytes) {
                        let image = image.to_rgba8();
                        let size = [image.width() as usize, image.height() as usize];
                        let pixels = image.as_flat_samples().as_slice().to_vec();
                        tx.send((pixels, size)).expect("Failed to send image data");
                    }
                }
            }
        });

        Self {
            background: None,
            receiver: rx,
        }
    }
}

impl App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        if self.background.is_none() {
            if let Ok((pixels, size)) = self.receiver.try_recv() {
                let texture = ctx.load_texture(
                    "background",
                    egui::ColorImage::from_rgba_unmultiplied(size, &pixels),
                    egui::TextureOptions::default(),
                );
                self.background = Some(texture);
                ctx.request_repaint();
            }
        }

        if let Some(bg) = &self.background {
            let painter = ctx.layer_painter(egui::LayerId::background());
            let rect = ctx.screen_rect();

            painter.image(
                bg.id(),
                rect,
                egui::Rect::from_min_max(rect.min, rect.max),
                egui::Color32::WHITE,
            );
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("3*4 Grid Example");
            egui::Grid::new("my_grid").show(ui, |ui| {
                for row in 0..3 {
                    for col in 0..4 {
                        ui.label(format!("R{} C{}", row + 1, col + 1));
                    }
                    ui.end_row();
                }
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "URL Background Example",
        options,
        Box::new(|_| Ok(Box::new(MyApp::new()))),
    )
}
