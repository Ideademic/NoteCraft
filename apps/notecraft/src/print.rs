//! Desktop printing (after WordCraft's adapter): craft-print finds the printers and sends File ›
//! Print's PDF to the spooler (CUPS `lp`, the Windows spooler). Neither ever runs on the UI
//! thread: each runs on a named worker and answers through a channel the Print page polls.

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use craft_ui::printing::{Discover, Duplex, Printer, Settings, Submit};
use notecraft_ui_egui::Printing;

/// How long quitting waits for jobs still being handed to the spooler.
const FINISH_WAIT: Duration = Duration::from_secs(10);

/// Submission workers, kept until they finish: dropping one at exit could end the hand-off
/// before the spooler has the job.
static JOBS: Mutex<Vec<JoinHandle<()>>> = Mutex::new(Vec::new());

fn jobs() -> MutexGuard<'static, Vec<JoinHandle<()>>> {
    JOBS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The printers, found on a worker thread.
pub fn discover() -> Discover {
    Box::new(|| {
        let (sender, receiver) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new().name("print-discovery".into()).spawn(move || {
            let printers = craft_print::printers().into_iter().map(|p| Printer { name: p.name, label: p.description, default: p.default }).collect();
            let _ = sender.send(printers);
        });
        if let Err(e) = spawned {
            // The sender is gone with the closure: the page reports that discovery failed.
            log::error!("print: no discovery thread: {e}");
        }
        receiver
    })
}

/// The spooler job for `settings`.
pub fn job(settings: &Settings) -> craft_print::Job {
    craft_print::Job {
        printer: Some(settings.printer.clone()).filter(|p| !p.is_empty()),
        copies: settings.copies.clamp(1, 999),
        collate: true,
        duplex: settings.duplex.map(|d| match d {
            Duplex::Off => craft_print::Duplex::Off,
            Duplex::LongEdge => craft_print::Duplex::LongEdge,
            Duplex::ShortEdge => craft_print::Duplex::ShortEdge,
        }),
        // NoteCraft's sheets are already the chosen paper size; a page wider than the printer's
        // paper still fits.
        scaling: craft_print::Scaling::ShrinkToFit,
        grayscale: settings.grayscale,
        title: "NoteCraft".into(),
        options: Vec::new(),
    }
}

/// Sends a PDF to the spooler on a worker thread.
pub fn submit() -> Submit {
    Box::new(|bytes: Vec<u8>, settings: Settings| {
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker = std::thread::Builder::new().name("print-submit".into()).spawn(move || {
            let sent = craft_print::submit(&bytes, &job(&settings)).map_err(|e| e.to_string());
            let _ = sender.send(sent);
        });
        match worker {
            Ok(w) => {
                let mut jobs = jobs();
                jobs.retain(|j| !j.is_finished());
                jobs.push(w);
            }
            Err(e) => log::error!("print: no submission thread: {e}"),
        }
        receiver
    })
}

/// The desktop's printing services.
pub fn printing() -> Printing {
    Printing::Spooler { discover: discover(), submit: submit() }
}

/// At exit: waits a while for jobs still being handed to the spooler.
pub fn finish_pending_jobs() {
    let pending = std::mem::take(&mut *jobs());
    let start = Instant::now();
    for job in pending {
        while !job.is_finished() && start.elapsed() < FINISH_WAIT {
            std::thread::sleep(Duration::from_millis(20));
        }
        if job.is_finished() {
            if job.join().is_err() {
                log::error!("print: a print worker stopped unexpectedly");
            }
        } else {
            log::warn!("print: a job was still being sent at exit");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jobs_follow_the_print_page() {
        let s = Settings { printer: "Office".into(), copies: 0, duplex: Some(Duplex::LongEdge), grayscale: true };
        let j = job(&s);
        assert_eq!(j.printer.as_deref(), Some("Office"));
        assert_eq!(j.copies, 1, "at least one copy");
        assert_eq!(j.duplex, Some(craft_print::Duplex::LongEdge));
        assert!(j.grayscale && j.collate);
        let j = job(&Settings { copies: 5000, ..Settings::default() });
        assert_eq!(j.printer, None, "no printer chosen: the default");
        assert_eq!(j.copies, 999);
        assert_eq!(j.duplex, None, "the printer's own default");
    }

    #[test]
    fn finishing_waits_for_workers() {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let w = std::thread::spawn(move || {
            let _ = rx.recv_timeout(Duration::from_millis(100));
        });
        jobs().push(w);
        drop(tx);
        finish_pending_jobs();
        assert!(jobs().is_empty());
    }
}
