mod app;
mod collect;
mod draw;
mod opts;
mod update;
mod widgets;

use std::fs;
use std::io::{self, Write};
use std::panic;
use std::path::Path;
use std::thread;
use std::time::Duration;

use clap::derive::Clap;
use crossbeam_channel::{select, tick, unbounded, Receiver};
use crossterm::cursor;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal;
use num_rational::Ratio;
use tui::backend::CrosstermBackend;
use tui::Terminal;

use app::*;
use draw::*;
use opts::Opts;
use update::*;

const PROGRAM_NAME: &str = env!("CARGO_PKG_NAME");

fn setup_terminal() {
    let mut stdout = io::stdout();

    execute!(stdout, terminal::EnterAlternateScreen).unwrap();
    execute!(stdout, cursor::Hide).unwrap();

    execute!(stdout, terminal::Clear(terminal::ClearType::All)).unwrap();

    terminal::enable_raw_mode().unwrap();
}

fn cleanup_terminal() {
    let mut stdout = io::stdout();

    execute!(stdout, cursor::MoveTo(0, 0)).unwrap();
    execute!(stdout, terminal::Clear(terminal::ClearType::All)).unwrap();

    execute!(stdout, terminal::LeaveAlternateScreen).unwrap();
    execute!(stdout, cursor::Show).unwrap();

    terminal::disable_raw_mode().unwrap();
}

fn setup_ui_events() -> Receiver<Event> {
    let (sender, receiver) = unbounded();
    thread::spawn(move || loop {
        sender.send(crossterm::event::read().unwrap()).unwrap();
    });

    receiver
}

fn setup_ctrl_c() -> Receiver<()> {
    let (sender, receiver) = unbounded();
    ctrlc::set_handler(move || {
        println!("press C-c");
        sender.send(()).unwrap();
    })
    .unwrap();

    receiver
}

fn setup_logfile(logfile_path: &Path) {
    fs::create_dir_all(logfile_path.parent().unwrap()).unwrap();
    let logfile = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(logfile_path)
        .unwrap();
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}]: {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ))
        })
        .chain(logfile)
        .level_for("mio", log::LevelFilter::Debug)
        .apply()
        .unwrap();
}

fn setup_panci_hook() {
    panic::set_hook(Box::new(|panic_info| {
        cleanup_terminal();
        better_panic::Settings::auto().create_panic_handler()(panic_info);
    }));
}

#[tokio::main]
async fn main() {
    better_panic::install();

    let opts: Opts = Opts::parse();

    let mut app = setup_app(&opts, PROGRAM_NAME);
    //setup_logfile(Path::new("./errors.log"));

    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).unwrap();

    setup_panci_hook();
    setup_terminal();

    let draw_interval = Ratio::from_integer(1);

    let ticker = tick(Duration::from_secs_f64(
        *draw_interval.numer() as f64 / *draw_interval.denom() as f64,
    ));

    let ui_event_receiver = setup_ui_events();
    let ctrl_c_events = setup_ctrl_c();

    let mut update_seconds = Ratio::from_integer(0);

    let collector = collect::Collector::new(app.urls.clone(), app.data.clone());
    tokio::spawn(collect::run(collector));

    update_widgets(&mut app.widgets, update_seconds);
    draw(&mut terminal, &mut app);

    loop {
        select! {
            recv(ctrl_c_events) -> _ => {
                break;
            }
            recv(ticker)->_ => {
                update_seconds = (update_seconds+draw_interval) % Ratio::from_integer(60);
                update_widgets(&mut app.widgets, update_seconds);
                draw(&mut terminal, &mut app);
            }
            recv(ui_event_receiver) -> message => {
                match message.unwrap() {
                    Event::Key(key_event) => {
                        if key_event.modifiers.is_empty() {
                            match key_event.code {
                                KeyCode::Char('q') => {
                                    break
                                }
                                _ => {}
                            }
                        } else if key_event.modifiers == KeyModifiers::CONTROL {
                            match key_event.code {
                                KeyCode::Char('c') => {
                                    break
                                }
                                _ => {}
                            }
                        }
                    }

                    Event::Resize(_width, _height) => {
                        draw(&mut terminal, &mut app);
                    }
                    _ => {}
                }
            }
        }
    }

    cleanup_terminal();
}
