// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod input_line;
mod save_output;
mod theme;
mod view;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("Uartist {}", uartist_core::VERSION))
            // Matches StartupWMClass in the .desktop file, so the desktop
            // shows the right icon and groups the windows.
            .with_app_id("uartist")
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!(
                    "../../../assets/icons/hicolor/256x256/apps/uartist.png"
                ))
                .expect("bundled icon is a valid PNG"),
            )
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([640.0, 400.0]),
        ..Default::default()
    };
    eframe::run_native(
        "uartist",
        options,
        Box::new(|cc| Ok(Box::new(app::UartistApp::new(cc)))),
    )
}
