mod draw;
mod opts;

use std::io::{self, Write};
use std::panic;
use std::thread;

use clap::derive::Clap;
use crossbeam_channel::{select, tick, unbounded, Receiver};
use crossterm::cursor;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal;
use tui::backend::CrosstermBackend;
use tui::Terminal;

use draw::*;
use opts::Opts;

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

fn setup_panci_hook() {
    panic::set_hook(Box::new(|panic_info| {
        cleanup_terminal();
        better_panic::Settings::auto().create_panic_handler()(panic_info);
    }));
}

fn main() {
    better_panic::install();

    let opts: Opts = Opts::parse();
    let urls: Vec<&str> = opts.url.as_str().split(',').collect();
    if urls.len() == 0 {
        println!("must set url");
        return;
    }

    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).unwrap();

    setup_panci_hook();
    setup_terminal();

    let ui_event_receiver = setup_ui_events();
    let ctrl_c_events = setup_ctrl_c();

    draw(&mut terminal);

    loop {
        select! {
            recv(ctrl_c_events) -> _ => {
                break;
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
                        draw(&mut terminal);
                    }
                    _ => {}
                }
            }
        }
    }

    cleanup_terminal();
}
