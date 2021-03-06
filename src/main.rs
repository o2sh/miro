#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate glium;
#[macro_use]
pub mod log;

use crate::window::glium_window::TerminalWindow;
use clap::{crate_description, crate_name, crate_version, App, AppSettings, Arg};
use config::{get_shell, Config, Theme};
use failure::Error;
use glium::glutin;
use mio::unix::EventedFd;
use mio::Events;
use mio::{Poll, PollOpt, Ready, Token};
use pty::openpty;
use std::env;
use std::os::unix::io::AsRawFd;
use std::process::Command;
use std::rc::Rc;
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{System, SystemExt};
use term::color::RgbColor;
use wakeup::{Wakeup, WakeupMsg};

mod config;
mod font;
mod opengl;
mod pty;
mod sigchld;
mod term;
mod wakeup;
mod window;

fn run(theme: Theme) -> Result<(), Error> {
    let config = Config::new(theme);
    let fontconfig = Rc::new(FontConfiguration::new(config.clone()));
    let event_loop = glutin::event_loop::EventLoop::<WakeupMsg>::with_user_event();
    let sys = System::new();

    let (wakeup_receiver, wakeup) = Wakeup::new(event_loop.create_proxy());
    sigchld::activate(wakeup.clone())?;

    let cmd = Command::new(get_shell()?);
    let font = fontconfig.default_font()?;
    let metrics = font.borrow_mut().get_fallback(0)?.metrics();

    let initial_cols = 80u16;
    let initial_rows = 24u16;
    let initial_pixel_width = initial_cols * metrics.cell_width.ceil() as u16;
    let initial_pixel_height = initial_rows * metrics.cell_height.ceil() as u16;

    let (master, slave) =
        openpty(initial_rows, initial_cols, initial_pixel_width, initial_pixel_height)?;

    let child = slave.spawn_command(cmd)?;

    let terminal = term::Terminal::new(
        initial_rows as usize,
        initial_cols as usize,
        config.scrollback_lines.unwrap_or(3500),
    );

    let master_fd = master.as_raw_fd();
    let mut window =
        TerminalWindow::new(&event_loop, wakeup_receiver, terminal, master, child, config, sys)?;
    {
        let mut wakeup = wakeup.clone();
        thread::spawn(move || {
            let poll = Poll::new().expect("mio Poll failed to init");
            poll.register(&EventedFd(&master_fd), Token(0), Ready::readable(), PollOpt::edge())
                .expect("failed to register pty");
            let mut events = Events::with_capacity(8);
            let mut last_paint = Instant::now();
            let refresh = Duration::from_millis(100);

            loop {
                let now = Instant::now();
                let diff = now - last_paint;
                let period = if diff >= refresh {
                    // Tick and wakeup the gui thread to ask it to render
                    // if needed.  Without this we'd only repaint when
                    // the window system decides that we were damaged.
                    // We don't want to paint after every state change
                    // as that would be too frequent.
                    wakeup.send(WakeupMsg::Paint).expect("failed to wakeup gui thread");
                    last_paint = now;
                    refresh
                } else {
                    refresh - diff
                };

                match poll.poll(&mut events, Some(period)) {
                    Ok(_) => {
                        for event in &events {
                            if event.token() == Token(0) && event.readiness().is_readable() {
                                wakeup
                                    .send(WakeupMsg::PtyReadable)
                                    .expect("failed to wakeup gui thread");
                            }
                        }
                    }
                    _ => {}
                }
            }
        });
    }

    event_loop.run(move |event, _, control_flow| match window.dispatch_event(event) {
        Ok(_) => return,
        Err(err) => {
            eprintln!("{:?}", err);
            *control_flow = glutin::event_loop::ControlFlow::Exit;
            return;
        }
    });
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
                .default_value("mario"),
        )
        .get_matches();

    let theme = match matches.value_of("theme") {
        Some("mario") => Theme {
            spritesheet_path: String::from("assets/gfx/mario.json"),
            header_color: RgbColor { red: 99, green: 137, blue: 250 },
        },
        Some("sonic") => Theme {
            spritesheet_path: String::from("assets/gfx/sonic.json"),
            header_color: RgbColor { red: 8, green: 129, blue: 0 },
        },
        Some("pika") => Theme {
            spritesheet_path: String::from("assets/gfx/pika.json"),
            header_color: RgbColor { red: 176, green: 139, blue: 24 },
        },
        Some("mega") => Theme {
            spritesheet_path: String::from("assets/gfx/mega.json"),
            header_color: RgbColor { red: 1, green: 135, blue: 147 },
        },
        Some("kirby") => Theme {
            spritesheet_path: String::from("assets/gfx/kirby.json"),
            header_color: RgbColor { red: 242, green: 120, blue: 141 },
        },
        _ => unreachable!("other values are not allowed"),
    };

    run(theme)
}
