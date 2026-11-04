// Where a frame goes: `lap("name")` adds the time since the previous lap (or `start`) to that name. A clock read per call, so the
// calls can stay in; `tests.rs` prints the totals (`take`).
use std::cell::RefCell;
use std::time::{Duration, Instant};

type Laps = (Option<Instant>, Vec<(&'static str, Duration)>);

thread_local! {
    static LAPS: RefCell<Laps> = const { RefCell::new((None, Vec::new())) };
}

pub fn start() {
    LAPS.with(|l| l.borrow_mut().0 = Some(Instant::now()));
}

pub fn lap(name: &'static str) {
    LAPS.with(|l| {
        let mut l = l.borrow_mut();
        let now = Instant::now();
        if let Some(t) = l.0 {
            let d = now - t;
            match l.1.iter_mut().find(|(n, _)| *n == name) {
                Some(e) => e.1 += d,
                None => l.1.push((name, d)),
            }
        }
        l.0 = Some(now);
    });
}

#[cfg(test)]
pub fn take() -> Vec<(&'static str, Duration)> {
    LAPS.with(|l| std::mem::take(&mut l.borrow_mut().1))
}
