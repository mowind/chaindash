mod app;
mod collect;
mod draw;
mod opts;
mod update;
mod widgets;

use std::{
    fs,
    io::{
        self,
        Write,
    },
    panic,
    path::Path,
    thread,
    time::Duration,
};

use app::*;
use clap::Parser;
use crossbeam_channel::{
    select,
    tick,
    unbounded,
    Receiver,
};
use crossterm::{
    cursor,
    event::{
        Event,
        KeyCode,
        KeyModifiers,
    },
    execute,
    terminal,
};
use draw::*;
//use log::{debug, info};
use num_rational::Ratio;
use opts::Opts;
use tui::{
    backend::CrosstermBackend,
    Terminal,
};
use update::*;

const PROGRAM_NAME: &str = env!("CARGO_PKG_NAME");

fn setup_terminal() -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = io::stdout();

    execute!(stdout, terminal::EnterAlternateScreen)?;
    execute!(stdout, cursor::Hide)?;

    execute!(stdout, terminal::Clear(terminal::ClearType::All))?;

    terminal::enable_raw_mode()?;

    Ok(())
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

fn setup_logfile(
    logfile_path: &Path,
    debug: bool,
) {
    let mut level = log::LevelFilter::Warn;
    if debug {
        level = log::LevelFilter::Debug;
    }

    fs::create_dir_all(logfile_path.parent().unwrap()).unwrap();
    let logfile =
        fs::OpenOptions::new().write(true).create(true).truncate(true).open(logfile_path).unwrap();
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
        .level(level)
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

    setup_logfile(Path::new("./errors.log"), opts.debug);

    let draw_interval = Ratio::min(Ratio::from_integer(1), opts.interval);

    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).unwrap();

    setup_panci_hook();
    if let Err(e) = setup_terminal() {
        eprintln!("Failed to setup terminal: {}", e);
        eprintln!(
            "This may be because you're running in an environment without a proper terminal."
        );
        eprintln!("Try running in a real terminal (not an IDE or pipe).");
        std::process::exit(1);
    }

    let ticker = tick(Duration::from_secs_f64(
        *draw_interval.numer() as f64 / *draw_interval.denom() as f64,
    ));

    let ui_event_receiver = setup_ui_events();
    let ctrl_c_events = setup_ctrl_c();

    let mut update_seconds = Ratio::from_integer(0);

    let collector = collect::Collector::new(&opts, app.data.clone());
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
                                KeyCode::Tab => {
                                    // Tab键切换磁盘
                                    app.handle_tab_key();
                                    draw(&mut terminal, &mut app);
                                }
                                _ => {}
                            }
                        } else if key_event.modifiers == KeyModifiers::SHIFT {
                            match key_event.code {
                                KeyCode::Tab => {
                                    // Shift+Tab键切换到上一个磁盘
                                    app.handle_shift_tab_key();
                                    draw(&mut terminal, &mut app);
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
