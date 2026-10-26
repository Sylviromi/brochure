//! Application entry point. Initializes the terminal, spawns the input thread and startup
//! background work, and drives the main MPSC event loop that coordinates Ratatui rendering
//! with input handling and background results.

mod app;
mod events;
mod fetch;
mod handlers;
mod models;
mod pixel_mouse;
mod saved_io;
mod state;
mod storage;
mod sync;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, MouseEventKind},
    execute,
    style::Print,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode, window_size,
    },
};
use models::{AddFeedStep, AppEvent, AppState};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    io,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

use crate::app::App;

/// Interval between animation ticks (spinners, scrolling titles).
const TICK: Duration = Duration::from_millis(250);

/// Upper bound on queued events handled before the next redraw, so a burst of
/// background results can't starve rendering.
const MAX_EVENTS_PER_FRAME: usize = 64;

/// Leave raw mode / alternate screen / mouse capture. Safe to call more than once.
fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(
        io::stdout(),
        Print(pixel_mouse::DISABLE),
        LeaveAlternateScreen,
        DisableMouseCapture
    );
}

/// Entry point. Sets up the terminal, runs the app, and always restores the terminal —
/// including on panic, so a crash never leaves the shell in raw mode.
#[tokio::main]
async fn main() -> Result<()> {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));

    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    // Pixel reports can only be mapped back to cells when the pixel size is known.
    let pixel_reports = window_size().is_ok_and(|s| s.width > 0 && s.height > 0);
    if pixel_reports {
        execute!(io::stdout(), Print(pixel_mouse::ENABLE))?;
    }
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    let result = run(&mut terminal, pixel_reports).await;

    restore_terminal();
    terminal.show_cursor()?;
    result
}

/// Query the terminal's image protocol, falling back to half-blocks.
///
/// When the terminal doesn't answer in time, ratatui-image silently returns its
/// default half-block picker but leaves its query thread blocked reading stdin until
/// a Device Status Report reply arrives — swallowing the user's first key press (seen
/// under tmux, whose wrapped passthrough queries may go unanswered). In that case ask
/// for a plain status report so the stuck thread consumes the terminal's reply
/// instead. If nothing is waiting, our input thread ignores the reply as unparseable.
fn detect_image_picker() -> limner::render_image::Picker {
    use limner::render_image::{Picker, ProtocolType};
    use std::io::Write;
    let picker =
        Picker::from_query_stdio().unwrap_or_else(|_| limner::render_image::halfblock_picker());
    let font = picker.font_size();
    let looks_like_timeout =
        picker.protocol_type() == ProtocolType::Halfblocks && (font.width, font.height) == (10, 20);
    if looks_like_timeout {
        let mut out = io::stdout();
        let _ = out.write_all(b"\x1b[5n").and_then(|()| out.flush());
        // Let the stuck reader consume the reply before our input thread starts.
        std::thread::sleep(Duration::from_millis(100));
    }
    picker
}

/// Forward terminal input to the event loop and emit a `Tick` every [`TICK`].
///
/// Plain pointer motion is dropped (it would trigger a redraw per move); presses,
/// releases, drags (feed editor drag-and-drop), and wheel turns are forwarded. With
/// `pixel_reports`, pointer positions arrive in pixels and are converted to cells here.
fn spawn_input_thread(tx: mpsc::UnboundedSender<AppEvent>, pixel_reports: bool) {
    std::thread::spawn(move || {
        let mut last_tick = Instant::now();
        let mut pixels = pixel_mouse::PixelMouse::default();
        loop {
            let timeout = TICK.saturating_sub(last_tick.elapsed());
            if event::poll(timeout).unwrap_or(false) {
                let forwarded = match event::read() {
                    Ok(Event::Key(key)) => Some(AppEvent::Input(key)),
                    Ok(Event::Mouse(mut m)) => {
                        // Motion is still converted: it is what confirms pixel reports.
                        let fine_row = window_size().ok().filter(|_| pixel_reports).and_then(|s| {
                            pixels.convert(&mut m, (s.columns, s.rows), (s.width, s.height))
                        });
                        matches!(
                            m.kind,
                            MouseEventKind::Down(_)
                                | MouseEventKind::Up(_)
                                | MouseEventKind::Drag(_)
                                | MouseEventKind::ScrollUp
                                | MouseEventKind::ScrollDown
                        )
                        .then_some(AppEvent::Mouse(m, fine_row))
                    }
                    // Redraw promptly at the new size.
                    Ok(Event::Resize(..)) => Some(AppEvent::Tick),
                    _ => None,
                };
                if let Some(ev) = forwarded
                    && tx.send(ev).is_err()
                {
                    return;
                }
            }
            if last_tick.elapsed() >= TICK {
                last_tick = Instant::now();
                if tx.send(AppEvent::Tick).is_err() {
                    return;
                }
            }
        }
    });
}

/// Main event loop. Loads cached articles, spawns background fetch tasks, and processes
/// input, background results, and ticks until the user quits.
async fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    pixel_reports: bool,
) -> Result<()> {
    let mut app = App::new();
    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();

    // Start network work first so it overlaps with terminal probing below.
    events::load_cached_articles(&mut app);
    events::start_initial_fetch(&mut app, &tx);
    {
        let tx = tx.clone();
        tokio::spawn(async move {
            if let Some(info) = fetch::check_latest_version().await {
                let _ = tx.send(AppEvent::UpdateAvailable(info));
            }
        });
    }

    // Detect the terminal image protocol before the input thread starts reading
    // stdin, so the terminal's reply isn't swallowed as key presses.
    app.picker = Some(detect_image_picker());
    spawn_input_thread(tx.clone(), pixel_reports);

    'main: loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;
        handlers::mouse::finish_selection_copy(&mut app);

        let Some(event) = rx.recv().await else { break };
        let mut next = Some(event);
        let mut handled = 0;
        while let Some(event) = next.take() {
            if handle_event(&mut app, event, &tx).await {
                break 'main;
            }
            handled += 1;
            if handled < MAX_EVENTS_PER_FRAME {
                next = rx.try_recv().ok();
            }
        }
        events::spawn_article_image_downloads(&mut app, &tx);
    }

    // Persist anything fetched since the last batch finished.
    let _ = storage::save_articles(&app.feeds, app.user_data.cache_full_articles);
    Ok(())
}

/// Apply one event to the app. Returns `true` when the user asked to quit.
async fn handle_event(
    app: &mut App,
    event: AppEvent,
    tx: &mpsc::UnboundedSender<AppEvent>,
) -> bool {
    match event {
        AppEvent::Input(key) => return handlers::handle_key(app, key, tx).await,
        AppEvent::Mouse(mouse, fine_row) => {
            return handlers::mouse::handle_mouse(app, mouse, fine_row, tx).await;
        }
        AppEvent::Tick => app.tick = app.tick.wrapping_add(1),
        AppEvent::FeedFetched(idx, result) => events::on_feed_fetched(app, idx, result),
        AppEvent::FreshRssFeedPulled(idx, result) => {
            sync::apply::on_freshrss_feed_pulled(app, idx, result);
        }
        AppEvent::FreshRssSynced(result) => sync::apply::on_freshrss_synced(app, result),
        AppEvent::FreshRssPushDone(result) => {
            if let Err(e) = result {
                app.notify(format!("⚠ FreshRSS push failed: {e}"));
            }
        }
        AppEvent::FeedTitleFetched(result) => {
            if app.state == AppState::AddFeed && app.add_feed.step == AddFeedStep::Title {
                app.add_feed.fetched_title = Some(result.unwrap_or_default());
            }
        }
        AppEvent::UpdateAvailable(info) => {
            app.update_popup_scroll = 0;
            app.update_available = Some(info);
        }
        AppEvent::ImageDecoded(url, image) => events::on_image_decoded(app, url, image),
        AppEvent::ZoomImageDecoded(url, image) => events::on_zoom_image(app, url, image),
        AppEvent::FullArticleFetched(source, art_idx, link, result) => {
            events::on_full_article_fetched(app, source, art_idx, &link, result);
        }
    }
    false
}
