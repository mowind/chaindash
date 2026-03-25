mod app;
mod collect;
mod draw;
mod error;
mod opts;
mod update;
mod widgets;

use std::{
    convert::TryFrom,
    fs,
    io::{
        self,
    },
    panic,
    path::Path,
    sync::Arc,
    thread,
    time::Duration,
};

use app::setup_app;
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
use draw::draw;
use error::ChaindashError;
use log::error;
//use log::{debug, info};
use num_rational::Ratio;
use opts::Opts;
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use update::update_widgets;

const PROGRAM_NAME: &str = env!("CARGO_PKG_NAME");

fn setup_terminal() -> Result<(), ChaindashError> {
    let mut stdout = io::stdout();

    execute!(stdout, terminal::EnterAlternateScreen)?;
    execute!(stdout, cursor::Hide)?;

    execute!(stdout, terminal::Clear(terminal::ClearType::All))?;

    terminal::enable_raw_mode()?;

    Ok(())
}

fn cleanup_terminal() {
    let mut stdout = io::stdout();

    let _ = execute!(stdout, cursor::MoveTo(0, 0));
    let _ = execute!(stdout, terminal::Clear(terminal::ClearType::All));
    let _ = execute!(stdout, terminal::LeaveAlternateScreen);
    let _ = execute!(stdout, cursor::Show);

    let _ = terminal::disable_raw_mode();
}

struct TerminalGuard {
    active: bool,
}

impl TerminalGuard {
    fn new() -> Self {
        Self { active: true }
    }

    fn cleanup(&mut self) {
        if self.active {
            cleanup_terminal();
            self.active = false;
        }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.cleanup();
    }
}

fn gcd_u128(
    mut left: u128,
    mut right: u128,
) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }

    left
}

fn lcm_u128(
    left: u128,
    right: u128,
) -> Result<u128, ChaindashError> {
    if left == 0 || right == 0 {
        return Ok(0);
    }

    let gcd = gcd_u128(left, right);
    left.checked_div(gcd)
        .and_then(|value| value.checked_mul(right))
        .ok_or_else(|| ChaindashError::Other("interval is too precise to schedule safely".into()))
}

fn scheduling_quantum(opts: &Opts) -> Result<Ratio<u64>, ChaindashError> {
    let intervals = [opts.interval, Ratio::from_integer(1), Ratio::from_integer(2)];
    let denominator_lcm = intervals
        .iter()
        .try_fold(1_u128, |acc, interval| lcm_u128(acc, u128::from(*interval.denom())))?;

    let numerator_gcd = intervals
        .iter()
        .map(|interval| {
            u128::from(*interval.numer()) * (denominator_lcm / u128::from(*interval.denom()))
        })
        .reduce(gcd_u128)
        .unwrap_or(1);

    let numerator = u64::try_from(numerator_gcd)
        .map_err(|_| ChaindashError::Other("interval numerator exceeds supported range".into()))?;
    let denominator = u64::try_from(denominator_lcm).map_err(|_| {
        ChaindashError::Other("interval denominator exceeds supported range".into())
    })?;

    Ok(Ratio::new(numerator, denominator))
}

fn setup_ui_events() -> Receiver<Event> {
    let (sender, receiver) = unbounded();
    thread::spawn(move || loop {
        match crossterm::event::read() {
            Ok(event) => {
                if sender.send(event).is_err() {
                    // Receiver dropped, exit thread
                    break;
                }
            },
            Err(e) => {
                log::error!("Failed to read terminal event: {}", e);
                break;
            },
        }
    });

    receiver
}

fn setup_ctrl_c() -> Result<Receiver<()>, ChaindashError> {
    let (sender, receiver) = unbounded();
    ctrlc::set_handler(move || {
        let _ = sender.send(());
    })
    .map_err(|e| ChaindashError::Ctrlc(e.to_string()))?;

    Ok(receiver)
}

fn setup_logfile(
    logfile_path: &Path,
    debug: bool,
) -> Result<(), ChaindashError> {
    let mut level = log::LevelFilter::Warn;
    if debug {
        level = log::LevelFilter::Debug;
    }

    // Handle case where path has no parent (e.g., "file.log" at root)
    if let Some(parent) = logfile_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let logfile =
        fs::OpenOptions::new().write(true).create(true).truncate(true).open(logfile_path)?;

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
        .apply()?;

    Ok(())
}

fn setup_panic_hook() {
    panic::set_hook(Box::new(|panic_info| {
        cleanup_terminal();
        better_panic::Settings::auto().create_panic_handler()(panic_info);
    }));
}

#[tokio::main]
async fn main() -> Result<(), ChaindashError> {
    better_panic::install();

    let opts: Opts = Opts::parse();
    if opts.interval == Ratio::from_integer(0) {
        return Err(ChaindashError::Other("interval must be greater than 0".to_string()));
    }

    let mut app = setup_app(&opts, PROGRAM_NAME);

    if let Err(e) = setup_logfile(Path::new("./errors.log"), opts.debug) {
        eprintln!("Failed to setup logfile: {}", e);
        // Continue without logging - not fatal
    }

    let draw_interval = scheduling_quantum(&opts)?;

    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);

    setup_panic_hook();

    let mut terminal =
        Terminal::new(backend).map_err(|e| ChaindashError::Terminal(e.to_string()))?;

    if let Err(e) = setup_terminal() {
        cleanup_terminal();
        eprintln!("Failed to setup terminal: {e}");
        eprintln!(
            "This may be because you're running in an environment without a proper terminal."
        );
        eprintln!("Try running in a real terminal (not an IDE or pipe).");
        return Err(e);
    }

    let mut terminal_guard = TerminalGuard::new();

    let ticker = tick(Duration::from_secs_f64(
        *draw_interval.numer() as f64 / *draw_interval.denom() as f64,
    ));

    let ui_event_receiver = setup_ui_events();
    let ctrl_c_events = setup_ctrl_c()?;

    let mut update_seconds = Ratio::from_integer(0);

    let collector = Arc::new(collect::Collector::new(&opts, app.data.clone())?);
    let collector_handle = {
        let collector_clone = Arc::clone(&collector);
        tokio::spawn(async move { collect::run(collector_clone).await })
    };

    update_widgets(&mut app.widgets, update_seconds);
    if let Err(err) = draw(&mut terminal, &mut app) {
        collector.stop();
        terminal_guard.cleanup();
        let _ = collector_handle.await;
        return Err(err);
    }

    'event_loop: loop {
        select! {
            recv(ctrl_c_events) -> _ => {
                break 'event_loop;
            }
            recv(ticker)->_ => {
                update_seconds += draw_interval;
                update_widgets(&mut app.widgets, update_seconds);
                if let Err(err) = draw(&mut terminal, &mut app) {
                    error!("绘制界面失败: {err}");
                    break 'event_loop;
                }
            }
            recv(ui_event_receiver) -> message => {
                let Ok(event) = message else {
                    // Channel closed, exit gracefully
                    break 'event_loop;
                };
                match event {
                    Event::Key(key_event) => {
                        match key_event.code {
                            KeyCode::BackTab => {
                                app.handle_shift_tab_key();
                                if let Err(err) = draw(&mut terminal, &mut app) {
                                    error!("绘制界面失败: {err}");
                                    break 'event_loop;
                                }
                            }
                            KeyCode::Char('q') if key_event.modifiers.is_empty() => {
                                break 'event_loop
                            }
                            KeyCode::Tab if key_event.modifiers.is_empty() => {
                                // Tab键切换磁盘
                                app.handle_tab_key();
                                if let Err(err) = draw(&mut terminal, &mut app) {
                                    error!("绘制界面失败: {err}");
                                    break 'event_loop;
                                }
                            }
                            KeyCode::Tab if key_event.modifiers == KeyModifiers::SHIFT => {
                                // Shift+Tab键切换到上一个磁盘
                                app.handle_shift_tab_key();
                                if let Err(err) = draw(&mut terminal, &mut app) {
                                    error!("绘制界面失败: {err}");
                                    break 'event_loop;
                                }
                            }
                            KeyCode::Char('c') if key_event.modifiers == KeyModifiers::CONTROL => {
                                break 'event_loop
                            }
                            _ => {}
                        }
                    }

                    Event::Resize(_width, _height) => {
                        if let Err(err) = draw(&mut terminal, &mut app) {
                            error!("绘制界面失败: {err}");
                            break 'event_loop;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    collector.stop();
    terminal_guard.cleanup();
    let collector_join_result = collector_handle.await;

    match collector_join_result {
        Ok(result) => result?,
        Err(err) => return Err(ChaindashError::Other(format!("collector task join error: {err}"))),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[test]
    fn test_scheduling_quantum_defaults_to_one_second_for_integer_intervals() {
        let opts = Opts::parse_from(["test", "--interval", "5"]);

        assert_eq!(
            scheduling_quantum(&opts).expect("quantum should be computed"),
            Ratio::from_integer(1)
        );
    }

    #[test]
    fn test_scheduling_quantum_supports_fractional_interval_above_one_second() {
        let opts = Opts::parse_from(["test", "--interval", "3/2"]);

        assert_eq!(
            scheduling_quantum(&opts).expect("quantum should be computed"),
            Ratio::new(1, 2)
        );
    }

    #[test]
    fn test_scheduling_quantum_supports_subsecond_interval() {
        let opts = Opts::parse_from(["test", "--interval", "2/3"]);

        assert_eq!(
            scheduling_quantum(&opts).expect("quantum should be computed"),
            Ratio::new(1, 3)
        );
    }
}
