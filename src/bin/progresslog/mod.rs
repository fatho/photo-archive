//! Module providing a combined interface to terminal logging via the `log` crate
//! and progress bars that can be interleaved with log messages.

use log::{Level, LevelFilter, Log, Metadata, Record};
use std::fmt::Display;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Logger that supports showing a progress bar while also still logging to stderr.
pub struct TermProgressLogger {
    level_filter: LevelFilter,
    term: console::Term,
    progress: ProgressLogger,
}

impl TermProgressLogger {
    pub fn new(level_filter: LevelFilter) -> TermProgressLogger {
        let term = console::Term::buffered_stderr();
        Self {
            level_filter,
            progress: ProgressLogger::new(term.clone()),
            term,
        }
    }

    pub fn init(level_filter: LevelFilter) -> Result<ProgressLogger, log::SetLoggerError> {
        let logger = Self::new(level_filter);
        let progress = logger.progress.clone();
        log::set_max_level(level_filter);
        log::set_boxed_logger(Box::new(logger))?;
        Ok(progress)
    }
}

impl Log for TermProgressLogger {
    /// Determines if a log message with the specified metadata would be
    /// logged.
    ///
    /// This is used by the `log_enabled!` macro to allow callers to avoid
    /// expensive computation of log message arguments if the message would be
    /// discarded anyway.
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level_filter
    }

    /// Logs the `Record`.
    ///
    /// Note that `enabled` is *not* necessarily called before this method.
    /// Implementations of `log` should perform all necessary filtering
    /// internally.
    fn log(&self, record: &Record) {
        use std::io::Write;

        if record.level() <= self.level_filter {
            let level = console::style(LevelDisplay(record.level()));
            let level_color = match record.level() {
                Level::Error => level.red(),
                Level::Warn => level.yellow(),
                _ => level.blue(),
            };

            // Ignore logging failures to stderr, as logging is not critical.
            let _ = self.progress.with_hidden_progress(|| {
                writeln!(
                    &self.term,
                    "{} {} {}",
                    console::style(chrono::Local::now().format("%F %T")).dim(),
                    level_color,
                    record.args()
                )?;
                self.term.flush()
            });
        }
    }

    /// Flushes any buffered records.
    fn flush(&self) {
        let _ = self.term.flush();
    }
}

pub struct AutoHideProgressBar {
    logger: ProgressLogger,
}

impl AutoHideProgressBar {
    pub fn sender(&self) -> ProgressSender {
        ProgressSender { progress_bar: self }
    }
}

impl Drop for AutoHideProgressBar {
    fn drop(&mut self) {
        self.logger.end_progress()
    }
}

pub struct ProgressSender<'a> {
    progress_bar: &'a AutoHideProgressBar,
}

impl<'a> ProgressSender<'a> {
    pub fn inc_progress(&self, amount: usize) {
        self.progress_bar.logger.inc_progress(amount);
    }
}

#[derive(Clone)]
pub struct ProgressLogger {
    current_progress: Option<Arc<Mutex<ProgressBarImpl>>>,
}

#[allow(unused)]
impl ProgressLogger {
    /// Create a new progress logger for the given terminal.
    /// If the terminal is not user attended, the progress bar won't render anything at all.
    fn new(term: console::Term) -> Self {
        Self {
            current_progress: if term.features().is_attended() {
                Some(Arc::new(Mutex::new(ProgressBarImpl::new(term))))
            } else {
                None
            },
        }
    }

    /// Hide the progress bar, then run the callback, then show the progress bar again if it was previously visible.
    fn with_hidden_progress<R, F: FnOnce() -> R>(&self, callback: F) -> std::io::Result<R> {
        if let Some(progress_impl) = self.current_progress.as_ref() {
            let mut progress_bar = progress_impl.lock().unwrap();
            let hide_and_restore = progress_bar.state == ProgressBarState::Visible;

            if hide_and_restore {
                progress_bar.clear()?;
            }
            let result = callback();
            if hide_and_restore {
                progress_bar.draw()?;
            }
            Ok(result)
        } else {
            Ok(callback())
        }
    }

    pub fn begin_progress(&self, total_progress: usize) -> AutoHideProgressBar {
        if let Some(progress_impl) = self.current_progress.as_ref() {
            let mut progress_bar = progress_impl.lock().unwrap();
            progress_bar.set_progress(0);
            progress_bar.set_total(total_progress);
            let _ = progress_bar.draw();
        }
        AutoHideProgressBar {
            logger: self.clone(),
        }
    }

    fn end_progress(&self) {
        if let Some(progress_impl) = self.current_progress.as_ref() {
            let mut progress_bar = progress_impl.lock().unwrap();
            let _ = progress_bar.clear();
        }
    }

    fn inc_progress(&self, delta: usize) {
        if let Some(progress_impl) = self.current_progress.as_ref() {
            let mut progress_bar = progress_impl.lock().unwrap();
            progress_bar.inc_progress(delta);
            let _ = progress_bar.refresh();
        }
    }

    fn inc_total(&self, delta: usize) {
        if let Some(progress_impl) = self.current_progress.as_ref() {
            let mut progress_bar = progress_impl.lock().unwrap();
            progress_bar.inc_total(delta);
            let _ = progress_bar.refresh();
        }
    }

    fn set_total(&self, total: usize) {
        if let Some(progress_impl) = self.current_progress.as_ref() {
            let mut progress_bar = progress_impl.lock().unwrap();
            progress_bar.set_total(total);
            let _ = progress_bar.refresh();
        }
    }

    fn set_progress(&self, progress: usize) {
        if let Some(progress_impl) = self.current_progress.as_ref() {
            let mut progress_bar = progress_impl.lock().unwrap();
            progress_bar.set_progress(progress);
            let _ = progress_bar.refresh();
        }
    }
}

/// Type for displaying a level within brackets.
struct LevelDisplay(Level);

impl Display for LevelDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use std::fmt::Write;

        f.write_char('[')?;
        self.0.fmt(f)?;
        f.write_char(']')
    }
}

struct ProgressBarImpl {
    term: console::Term,
    total_progress: usize,
    current_progress: usize,
    state: ProgressBarState,
    last_update: Instant,
}

#[derive(Debug, Eq, PartialEq)]
enum ProgressBarState {
    Hidden,
    Visible,
}

impl ProgressBarImpl {
    pub fn new(term: console::Term) -> Self {
        Self {
            term,
            total_progress: 0,
            current_progress: 0,
            state: ProgressBarState::Hidden,
            last_update: Instant::now(),
        }
    }

    pub fn inc_progress(&mut self, delta: usize) {
        self.current_progress = self
            .current_progress
            .saturating_add(delta)
            .min(self.total_progress);
    }

    pub fn inc_total(&mut self, delta: usize) {
        self.total_progress = self.total_progress.saturating_add(delta);
    }

    pub fn set_total(&mut self, total: usize) {
        self.total_progress = total;
        self.current_progress = self.current_progress.min(total);
    }

    pub fn set_progress(&mut self, progress: usize) {
        self.current_progress = progress.min(self.total_progress);
    }

    /// Hide the progress bar and set the cursor to where it was drawn.
    pub fn clear(&mut self) -> std::io::Result<()> {
        if self.state == ProgressBarState::Visible {
            self.term.clear_last_lines(1)?;
            self.term.flush()?;
            self.state = ProgressBarState::Hidden;
        }
        Ok(())
    }

    fn check_rate_limit(&mut self) -> bool {
        let now = Instant::now();
        if now.duration_since(self.last_update) > Duration::from_millis(100) {
            self.last_update = now;
            true
        } else {
            false
        }
    }

    /// Draw the progress bar like this: ` [=========>         ] 10/20 `
    pub fn draw(&mut self) -> std::io::Result<()> {
        if self.state == ProgressBarState::Visible && !self.check_rate_limit() {
            // Do not render if the progress bar is already visible but we hit the rate limiting.
            return Ok(());
        }

        let (_height, width) = self.term.size();

        // First compute the textutal part of the progress indicator
        let progress_text = format!("{}/{}", self.current_progress, self.total_progress);
        let progress_text_width = console::measure_text_width(&progress_text);

        // Then use the remaining space for drawing the bar
        let remaining = (width as usize).saturating_sub(progress_text_width + 7);
        let mut bar_text = String::new();

        if remaining > 0 {
            bar_text.push(' ');
            bar_text.push('[');
            let pos =
                (self.current_progress * remaining / self.total_progress.max(1)).min(remaining);
            for _ in 0..pos {
                bar_text.push('=')
            }
            if pos < remaining {
                bar_text.push('>');
            }
            for _ in pos + 1..remaining {
                bar_text.push(' ');
            }
            bar_text.push(']');
        }
        bar_text.push(' ');
        bar_text.push_str(&progress_text);
        let line = console::truncate_str(&bar_text, width as usize, "...");

        // If the bar was shown previously, move the cursor up for updating it
        if self.state == ProgressBarState::Visible {
            self.term.move_cursor_up(1)?;
        }

        self.term.write_line(&line)?;
        self.state = ProgressBarState::Visible;

        self.term.flush()
    }

    /// Like `draw`, but only printing if the bar is already visible.
    pub fn refresh(&mut self) -> std::io::Result<()> {
        if self.state == ProgressBarState::Visible {
            self.draw()
        } else {
            Ok(())
        }
    }
}
