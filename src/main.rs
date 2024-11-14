use std::time::Duration;
use std::thread;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use signal_hook::consts::TERM_SIGNALS;
use anyhow::Result;

mod backlight;

/// Checks the ambient light sensor sysfs file at a set interval, if the relative change is greater
/// than 25% it sends a message containing the measured value.
///
/// TODO: add config file options for the check interval and desired ratios, move into Backlight?
fn ambient_watcher(path: String, term: Arc<AtomicBool>) -> Result<Receiver<i32>> {
    let (tx, rx) = channel::<i32>();
    thread::spawn(move || {
        let mut previous = backlight::read_sys_file(&path).unwrap();
        while !term.load(Ordering::Relaxed) {
            let current = backlight::read_sys_file(&path).unwrap();
            let ratio = (previous - current).abs() * 100 / previous.max(1);
            if ratio > 25 {
                previous = current;
                tx.send(current).unwrap();
            }
            thread::sleep(Duration::from_millis(1000));
        }
    });
    Ok(rx)
}

fn main() -> Result<()> {
    // Gracefully shutdown on SIGTERM, SIGQUIT, and SIGINT signals
    // Forcibly terminates after receiving a second termination signal
    let term = Arc::new(AtomicBool::new(false));
    for sig in TERM_SIGNALS {
        signal_hook::flag::register_conditional_shutdown(*sig, 1, Arc::clone(&term))?;
        signal_hook::flag::register(*sig, Arc::clone(&term))?;
    }

    // Create a config using defaults
    let config = backlight::Config::default();

    let backlight = backlight::Backlight::new(config)?;

    let change = ambient_watcher(backlight.config.ambient_path.clone(),Arc::clone(&term))?;
    while let Ok(msg) = change.recv() {
        backlight.set_brightness(msg, Arc::clone(&term))?;
    }
    println!("");
    Ok(())
}
