pub mod tracer;

use std::{collections::HashMap, net::IpAddr, sync::{Arc, Mutex}};

use tracer::{TraceHandler, TraceState};
use walkers::{lon_lat, sources::OpenStreetMap, HttpTiles, Map, MapMemory, Plugin, Position, extras::{Places}};
use egui::{CollapsingHeader, Context, RichText, SidePanel, Slider};
use eframe::{App, CreationContext, Frame};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default(),
        ..Default::default()
    };
    eframe::run_native(
        "gtraceroute",
        options,
        Box::new(|cc| {
            Ok(Box::new(GeoTrace::new(cc)))
        }),
    )
}


struct GeoTrace {
    tiles: HttpTiles,
    map_memory: MapMemory,
    host: String,
    geo_cache: HashMap<String, Position>,
    use_cache: bool,
    max_hops: usize,
    tracer: TraceHandler,
    state: Arc<Mutex<TraceState>>
}

impl GeoTrace {
    fn new(cc: &CreationContext) -> Self {
        let mut memory = MapMemory::default();
        if memory.set_zoom(1.0).is_err() {
            println!("failed to set zoom level!");
        }

        let state: TraceState = Default::default(); 

        let state_arc = Arc::new(Mutex::new(state));
        let state_arc_clone = Arc::clone(&state_arc);
        let ctx_clone = cc.egui_ctx.clone();

        return Self {
            tiles: HttpTiles::new(OpenStreetMap, cc.egui_ctx.clone()),
            map_memory: memory,
            host: String::from("1.1.1.1"),
            geo_cache: Default::default(),
            use_cache: true,
            max_hops: 30,
            tracer: TraceHandler::new(move |state| {
                ctx_clone.request_repaint();
                let mut lock = state_arc.lock().unwrap();
                *lock = state;
            }),
            state: state_arc_clone,
            
        }
    }

    fn places(&self) -> impl Plugin + use<> {
        return Places::new(vec![]);
    }

    fn toggle_tracer(&mut self) {
        if self.tracer.is_tracing() {
            self.tracer.stop_trace();
        } else {
            self.tracer.begin_trace();
        }
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
                        

                        let button_text = match self.tracer.is_tracing() {
                                true => "Stop tracing",
                                false => "Start trace"
                            };

                        if ui.button(button_text).clicked() {
                            if let Ok(ip) = self.host.parse::<IpAddr>() {
                                self.tracer.set_ip(ip);
                                self.toggle_tracer();
                            } else {
                                println!("invalid ip"); // todo: make errors prettier
                            }
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

                ui.add_space(20.0);

                //ui.label(format!("{}", self.state.lock().unwrap().counter))
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
