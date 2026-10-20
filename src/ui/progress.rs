//! A small spinning circle with a live counter, shown while long work runs.
//!
//! It is drawn on stderr by a background thread, so it keeps turning even
//! while a single big file is being read. It only appears on a real terminal;
//! when output goes to a file or a pipe nothing is drawn.

use std::io::{self, IsTerminal, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use crate::ui::style::dim;

const FRAMES: [&str; 4] = ["◐", "◓", "◑", "◒"];
const TICK: Duration = Duration::from_millis(100);

/// Spinner shown until it is dropped. Feed it progress with `update`,
/// e.g. by passing `&|done, total| spinner.update(done, total)` to a `core` function.
pub struct Spinner {
    done: Arc<AtomicUsize>,
    total: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Spinner {
    /// Starts showing `◐ <label> <count> <unit>`. With a known total the
    /// count reads "340 of 1555", otherwise just "340".
    pub fn start(label: &str, unit: &str) -> Spinner {
        let done = Arc::new(AtomicUsize::new(0));
        let total = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));

        let handle = io::stderr().is_terminal().then(|| {
            let (done, total, stop) = (done.clone(), total.clone(), stop.clone());
            let (label, unit) = (label.to_string(), unit.to_string());
            thread::spawn(move || {
                let mut frame = 0;
                while !stop.load(Ordering::Relaxed) {
                    let (n, t) = (done.load(Ordering::Relaxed), total.load(Ordering::Relaxed));
                    let count = if t > 0 { format!("{} of {} {}", n, t, unit) } else { format!("{} {}", n, unit) };
                    // \r + "clear line" redraws in place
                    eprint!("\r\x1b[2K{} {}  {}", FRAMES[frame % FRAMES.len()], label, dim(&count));
                    let _ = io::stderr().flush();
                    frame += 1;
                    thread::sleep(TICK);
                }
                eprint!("\r\x1b[2K");
                let _ = io::stderr().flush();
            })
        });

        Spinner { done, total, stop, handle }
    }

    /// Records progress; the next frame shows it.
    pub fn update(&self, done: usize, total: usize) {
        self.done.store(done, Ordering::Relaxed);
        self.total.store(total, Ordering::Relaxed);
    }
}

impl Drop for Spinner {
    /// Stops the animation and clears its line, so results print on a clean line.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
