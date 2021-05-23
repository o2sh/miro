#[macro_use]
extern crate failure;

use clap::{crate_description, crate_name, crate_version, App, AppSettings, Arg};
use failure::Error;
use std::rc::Rc;
use std::sync::Arc;

use crate::config::Theme;
use crate::font::FontConfiguration;
use crate::mux::Mux;
use crate::pty::PtySize;
use crate::term::color::RgbColor;

mod config;
mod core;
mod font;
mod gui;
mod mux;
mod pty;
mod term;
mod window;

fn run(theme: Theme) -> Result<(), Error> {
    let config = Arc::new(config::Config::default_config(theme));
    let fontconfig = Rc::new(FontConfiguration::new(Arc::clone(&config)));
    let gui = gui::new()?;
    let mux = Rc::new(mux::Mux::new(&config, PtySize::default())?);
    Mux::set_mux(&mux);

    mux.start()?;

    gui.spawn_new_window(&fontconfig)?;

    gui.run_forever()
}

fn main() -> Result<(), Error> {
    let matches = App::new(crate_name!())
        .version(crate_version!())
        .about(crate_description!())
        .setting(AppSettings::ColoredHelp)
        .setting(AppSettings::DeriveDisplayOrder)
        .setting(AppSettings::UnifiedHelpMessage)
        .setting(AppSettings::HidePossibleValuesInHelp)
        .arg(
            Arg::with_name("theme")
                .short("t")
                .long("theme")
                .help("Which theme to use (pika, kirby, *mario*).")
                .possible_values(&["mario", "pika", "kirby"])
                .default_value("mario")
                .hide_default_value(true)
                .takes_value(true),
        )
        .get_matches();
    let theme = match matches.value_of("theme") {
        Some("mario") => Theme {
            spritesheet_path: String::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/gfx/mario.json"
            )),
            color: RgbColor { red: 99, green: 137, blue: 250 },
        },
        Some("pika") => Theme {
            spritesheet_path: String::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/gfx/pika.json"
            )),
            color: RgbColor { red: 176, green: 139, blue: 24 },
        },
        Some("kirby") => Theme {
            spritesheet_path: String::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/gfx/kirby.json"
            )),
            color: RgbColor { red: 242, green: 120, blue: 141 },
        },
        _ => unreachable!("not possible"),
    };

    run(theme)
}
