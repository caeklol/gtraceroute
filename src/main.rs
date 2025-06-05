use std::collections::HashMap;

use walkers::{lon_lat, sources::OpenStreetMap, HttpTiles, Map, MapMemory, Plugin, Position, extras::{Places}};
use egui::{CollapsingHeader, Context, RichText, SidePanel, Slider};
use eframe::{App, CreationContext, Frame};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default(),
        ..Default::default()
    };
    eframe::run_native(
        "My egui App",
        options,
        Box::new(|cc| {
            Ok(Box::new(GeoTrace::new(cc)))
        }),
    )
}

struct TraceHandler {
    tracing: bool,

}

impl TraceHandler {
    
}

struct GeoTrace {
    tiles: HttpTiles,
    map_memory: MapMemory,
    host: String,
    geo_cache: HashMap<String, Position>,
    use_cache: bool,
    max_hops: usize,
    tracer: TraceHandler
}

impl GeoTrace {
    fn new(cc: &CreationContext) -> Self {
        let mut memory = MapMemory::default();
        if memory.set_zoom(1.0).is_err() {
            println!("failed to set zoom level!");
        }

        return Self {
            tiles: HttpTiles::new(OpenStreetMap, cc.egui_ctx.clone()),
            map_memory: memory,
            host: String::from("google.com"),
            geo_cache: Default::default(),
            use_cache: true,
            max_hops: 30
        }
    }

    fn places(&self) -> impl Plugin + use<> {
        return Places::new(vec![]);
    }

    fn toggle_trace(&mut self) {
    }
}

impl App for GeoTrace {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        SidePanel::left("control_panel")
            .resizable(true)
            .min_width(300.0)
            .show(ctx, |ui| {
                CollapsingHeader::new("Trace")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("traceopts")
                            .num_columns(2)
                            .show(ui, |ui| {
                                let label = ui.label("Host: ");
                                ui.text_edit_singleline(&mut self.host)
                                    .labelled_by(label.id);
                                ui.end_row();

                                let label = ui.label("Max hops: ");
                                ui.add(Slider::new(&mut self.max_hops, 1..=100))
                                    .labelled_by(label.id);
                                ui.end_row();
                            });
                        

                        if ui.button("Start trace").clicked() {

                        }

                        ui.add_space(5.0);
                    });

                ui.add_space(12.0);

                CollapsingHeader::new("Geolocation")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.button("Clear cache").clicked() {
                                self.geo_cache = HashMap::new();
                            }
                            ui.toggle_value(&mut self.use_cache, "Use cache");
                        });

                        ui.label(RichText::new(format!("Cache size: {}", &self.geo_cache.len())).monospace());

                        ui.add_space(5.0);
                        ui.label("After tracing, the IPs of each hop are geolocated. To disable caching of these results, use the controls above.");
                    });
            });

        SidePanel::right("map")
            .resizable(false)
            .show_separator_line(false) // i dont know if this is redundant and im too lazy to
                                        // check if it is :)
            .exact_width(ctx.available_rect().x_range().max)
            .show(ctx, |ui| {
                let plugin = self.places();
                let map = Map::new(
                    Some(&mut self.tiles),
                    &mut self.map_memory,
                    lon_lat(0.0, 0.0),
                ).with_plugin(plugin);

                ui.add(map);
            });
    }
}
