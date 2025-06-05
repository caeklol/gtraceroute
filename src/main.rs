use walkers::{HttpTiles, Map, MapMemory, Position, sources::OpenStreetMap, lon_lat};
use egui::{Context, FontId, RichText, SidePanel};
use eframe::{App, CreationContext, Frame};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([640.0, 400.0]),
        ..Default::default()
    };
    eframe::run_native(
        "My egui App",
        options,
        Box::new(|cc| {
            Ok(Box::new(MyApp::new(cc)))
        }),
    )
}

struct MyApp {
    tiles: HttpTiles,
    map_memory: MapMemory,
    host: String
}

impl MyApp {
    fn new(cc: &CreationContext) -> Self {
        Self {
            tiles: HttpTiles::new(OpenStreetMap, cc.egui_ctx.clone()),
            map_memory: MapMemory::default(),
            host: String::from("google.com")
        }
    }
}

impl App for MyApp {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        ctx.set_zoom_factor(1.2);
        SidePanel::left("control_panel")
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.label("Host: ");
                    ui.text_edit_singleline(&mut self.host)
                        .labelled_by(label.id)
                });
            });

        SidePanel::right("map")
            .resizable(false)
            .min_width(ctx.available_rect().x_range().max)
            .show(ctx, |ui| {
                ui.add(Map::new(
                    Some(&mut self.tiles),
                    &mut self.map_memory,
                    lon_lat(0.0, 0.0)
                ));

            });
    }
}
