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

fn run(theme: Option<Theme>) -> Result<(), Error> {
    let config = Arc::new(config::Config::default_config(theme));
    let fontconfig = Rc::new(FontConfiguration::new(Arc::clone(&config)));

    let mux = Rc::new(mux::Mux::new(&config, PtySize::default())?);
    Mux::set_mux(&mux);
    mux.start()?;

    let gui = gui::new()?;

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
        .arg(
            Arg::with_name("theme")
                .short("t")
                .long("theme")
                .help("Which theme to use.")
                .possible_values(&["mario", "sonic", "pika", "mega", "kirby"])
                .takes_value(true),
        )
        .get_matches();

    let theme = match matches.value_of("theme") {
        Some("sonic") => Some(Theme {
            spritesheet_path: String::from("assets/sonic.json"),
            header_color: RgbColor { red: 8, green: 129, blue: 0 },
        }),
        Some("pika") => Some(Theme {
            spritesheet_path: String::from("assets/pika.json"),
            header_color: RgbColor { red: 176, green: 139, blue: 24 },
        }),
        Some("mega") => Some(Theme {
            spritesheet_path: String::from("assets/mega.json"),
            header_color: RgbColor { red: 1, green: 135, blue: 147 },
        }),
        Some("kirby") => Some(Theme {
            spritesheet_path: String::from("assets/kirby.json"),
            header_color: RgbColor { red: 242, green: 120, blue: 141 },
        }),
        _ => None,
    };

    run(theme)
}
