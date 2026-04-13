mod app;
mod collect;
mod draw;
mod error;
mod notify;
mod opts;
mod sync;
mod update;
mod widgets;

use std::{
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

use app::{
    setup_app,
    App,
};
use clap::Parser;
use crossbeam_channel::{
    bounded,
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
        KeyEvent,
        KeyModifiers,
    },
    execute,
    terminal,
};
use draw::draw;
use error::ChaindashError;
use log::error;
use num_rational::Ratio;
use opts::Opts;
use ratatui::{
    backend::{
        Backend,
        CrosstermBackend,
    },
    Terminal,
};
use sync::lock_or_panic;
use update::update_widgets;

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

const PERIODIC_REDRAW_INTERVAL: Duration = Duration::from_secs(1);

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

fn draw_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> error::Result<()> {
    {
        let mut data = lock_or_panic(&app.data);
        data.expire_status_message_if_needed();
    }

    draw(terminal, app)
}

enum UiAction {
    None,
    Redraw,
    Exit,
}

fn draw_or_capture_exit<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    exit_error: &mut Option<ChaindashError>,
) -> bool {
    if let Err(err) = draw_app(terminal, app) {
        error!("绘制界面失败: {err}");
        *exit_error = Some(err);
        true
    } else {
        false
    }
}

fn is_quit_key(key_event: &KeyEvent) -> bool {
    key_event.code == KeyCode::Char('q') && key_event.modifiers.is_empty()
}

fn is_ctrl_c(key_event: &KeyEvent) -> bool {
    key_event.code == KeyCode::Char('c') && key_event.modifiers == KeyModifiers::CONTROL
}

fn is_tab(key_event: &KeyEvent) -> bool {
    key_event.code == KeyCode::Tab && key_event.modifiers.is_empty()
}

fn is_shift_tab(key_event: &KeyEvent) -> bool {
    key_event.code == KeyCode::BackTab
        || (key_event.code == KeyCode::Tab && key_event.modifiers == KeyModifiers::SHIFT)
}

fn handle_ui_event(
    app: &mut App,
    event: Event,
) -> UiAction {
    match event {
        Event::Key(key_event) if is_shift_tab(&key_event) => {
            if app.handle_shift_tab_key() {
                UiAction::Redraw
            } else {
                UiAction::None
            }
        },
        Event::Key(key_event) if is_quit_key(&key_event) => UiAction::Exit,
        Event::Key(key_event) if is_tab(&key_event) => {
            if app.handle_tab_key() {
                UiAction::Redraw
            } else {
                UiAction::None
            }
        },
        Event::Key(key_event) if is_ctrl_c(&key_event) => UiAction::Exit,
        Event::Resize(_, _) => UiAction::Redraw,
        _ => UiAction::None,
    }
}

#[tokio::main]
async fn main() -> Result<(), ChaindashError> {
    better_panic::install();

    let opts: Opts = Opts::parse();

    let mut app = setup_app(&opts);

    if let Err(e) = setup_logfile(Path::new("./errors.log"), opts.debug) {
        eprintln!("Failed to setup logfile: {}", e);
        // Continue without logging - not fatal
    }

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

    let ticker = tick(PERIODIC_REDRAW_INTERVAL);

    let ui_event_receiver = setup_ui_events();
    let ctrl_c_events = setup_ctrl_c()?;
    let (ui_refresh_sender, ui_refresh_receiver) = bounded(1);
    app.install_ui_waker(ui_refresh_sender);

    let collector = Arc::new(collect::Collector::new(&opts, app.data.clone())?);
    let collector_handle = {
        let collector_clone = Arc::clone(&collector);
        tokio::spawn(async move { collect::run(collector_clone).await })
    };

    update_widgets(&mut app.widgets, Ratio::from_integer(0));
    if let Err(err) = draw_app(&mut terminal, &mut app) {
        collector.stop();
        terminal_guard.cleanup();
        let _ = collector_handle.await;
        return Err(err);
    }

    let mut exit_error = None;

    'event_loop: loop {
        select! {
            recv(ctrl_c_events) -> _ => {
                break 'event_loop;
            }
            recv(ticker)->_ => {
                if app.needs_periodic_redraw()
                    && draw_or_capture_exit(&mut terminal, &mut app, &mut exit_error)
                {
                    break 'event_loop;
                }
            }
            recv(ui_refresh_receiver) -> message => {
                let Ok(()) = message else {
                    break 'event_loop;
                };

                if app.refresh_dirty_widgets()
                    && draw_or_capture_exit(&mut terminal, &mut app, &mut exit_error)
                {
                    break 'event_loop;
                }
            }
            recv(ui_event_receiver) -> message => {
                let Ok(event) = message else {
                    // Channel closed, exit gracefully
                    break 'event_loop;
                };

                match handle_ui_event(&mut app, event) {
                    UiAction::None => {}
                    UiAction::Redraw => {
                        if draw_or_capture_exit(&mut terminal, &mut app, &mut exit_error) {
                            break 'event_loop;
                        }
                    }
                    UiAction::Exit => break 'event_loop,
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

    if let Some(err) = exit_error {
        return Err(err);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use clap::Parser;

    use super::*;
    use crate::collect::DiskDetail;

    fn create_test_app() -> App {
        let opts = Opts::parse_from(["test", "--url", "test@ws://127.0.0.1:6789"]);
        setup_app(&opts)
    }

    #[cfg(target_family = "unix")]
    fn create_test_disk_detail(mount_point: &str) -> DiskDetail {
        DiskDetail {
            mount_point: mount_point.to_string(),
            filesystem: "ext4".to_string(),
            total: 100_000_000_000,
            used: 50_000_000_000,
            available: 50_000_000_000,
            usage_percent: 50.0,
            device: "/dev/sda1".to_string(),
            is_alert: false,
            is_network: false,
            last_updated: Instant::now(),
        }
    }

    #[test]
    fn test_periodic_redraw_interval_is_one_second() {
        assert_eq!(PERIODIC_REDRAW_INTERVAL, Duration::from_secs(1));
    }

    #[test]
    fn test_handle_ui_event_returns_exit_for_quit_key() {
        let mut app = create_test_app();
        let event = Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));

        assert!(matches!(handle_ui_event(&mut app, event), UiAction::Exit));
    }

    #[test]
    fn test_handle_ui_event_returns_redraw_for_resize() {
        let mut app = create_test_app();

        assert!(matches!(handle_ui_event(&mut app, Event::Resize(120, 40)), UiAction::Redraw));
    }

    #[cfg(target_family = "unix")]
    #[test]
    fn test_handle_ui_event_returns_redraw_for_tab_with_disk_data() {
        let mut app = create_test_app();
        {
            let mut data = app.data.lock().expect("mutex poisoned");
            data.set_disk_details_for_test(vec![
                create_test_disk_detail("/"),
                create_test_disk_detail("/home"),
            ]);
        }

        let event = Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

        assert!(matches!(handle_ui_event(&mut app, event), UiAction::Redraw));
        assert_eq!(app.widgets.disk_list.current_disk_index_for_test(), 1);
    }

    #[cfg(target_family = "unix")]
    #[test]
    fn test_handle_ui_event_returns_none_for_tab_with_single_disk() {
        let mut app = create_test_app();
        {
            let mut data = app.data.lock().expect("mutex poisoned");
            data.set_disk_details_for_test(vec![create_test_disk_detail("/")]);
        }

        let event = Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

        assert!(matches!(handle_ui_event(&mut app, event), UiAction::None));
        assert_eq!(app.widgets.disk_list.current_disk_index_for_test(), 0);
    }

    #[cfg(target_family = "unix")]
    #[test]
    fn test_handle_ui_event_returns_redraw_for_backtab_with_disk_data() {
        let mut app = create_test_app();
        {
            let mut data = app.data.lock().expect("mutex poisoned");
            data.set_disk_details_for_test(vec![
                create_test_disk_detail("/"),
                create_test_disk_detail("/home"),
            ]);
        }

        let event = Event::Key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));

        assert!(matches!(handle_ui_event(&mut app, event), UiAction::Redraw));
        assert_eq!(app.widgets.disk_list.current_disk_index_for_test(), 1);
    }

    #[cfg(target_family = "unix")]
    #[test]
    fn test_handle_ui_event_returns_redraw_for_shift_tab_with_disk_data() {
        let mut app = create_test_app();
        {
            let mut data = app.data.lock().expect("mutex poisoned");
            data.set_disk_details_for_test(vec![
                create_test_disk_detail("/"),
                create_test_disk_detail("/home"),
            ]);
        }

        let event = Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT));

        assert!(matches!(handle_ui_event(&mut app, event), UiAction::Redraw));
        assert_eq!(app.widgets.disk_list.current_disk_index_for_test(), 1);
    }
}
