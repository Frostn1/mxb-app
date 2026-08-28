//! One build per key at a time.
//!
//! A cache in front of expensive work only helps the caller that arrives second. Two that
//! arrive together both miss, both pay, and the loser's result is displaced from the cache
//! the moment it inserts — so the duplicate work is worse than wasted, it strands the pixels
//! hanging off whichever model lost. The viewer really does ask twice: a panel and a dialog
//! can draw the same bike, and an effect can re-fire before the first answer lands.
//!
//! Callers check their cache, take the gate, then check again. Whoever waited finds the
//! answer the first one just put there.

use std::collections::HashSet;
use std::sync::{Condvar, Mutex, OnceLock};

#[derive(Default)]
struct Gates {
    building: Mutex<HashSet<String>>,
    done: Condvar,
}

fn gates() -> &'static Gates {
    static G: OnceLock<Gates> = OnceLock::new();
    G.get_or_init(Gates::default)
}

/// Held while its key is being built. Dropping it — including on the `?` out of a build that
/// failed — lets the next caller through.
pub struct Gate(String);

impl Drop for Gate {
    fn drop(&mut self) {
        let g = gates();
        // Poisoned means a build panicked while holding the set, not that the set is wrong.
        let mut set = g.building.lock().unwrap_or_else(|p| p.into_inner());
        set.remove(&self.0);
        drop(set);
        g.done.notify_all();
    }
}

/// Wait until nobody else is building `key`, then claim it.
///
/// Re-check the cache once this returns: the whole point of having waited is that the build
/// you were queued behind has just finished.
pub fn enter(key: &str) -> Gate {
    let g = gates();
    let mut set = g.building.lock().unwrap_or_else(|p| p.into_inner());
    while set.contains(key) {
        set = g.done.wait(set).unwrap_or_else(|p| p.into_inner());
    }
    set.insert(key.to_string());
    Gate(key.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn the_second_caller_waits_and_takes_the_first_one_s_answer() {
        static CACHE: Mutex<Option<u32>> = Mutex::new(None);
        static BUILDS: AtomicUsize = AtomicUsize::new(0);

        let callers: Vec<_> = (0..4)
            .map(|_| {
                std::thread::spawn(|| {
                    if let Some(v) = *CACHE.lock().expect("cache") {
                        return v;
                    }
                    let _gate = enter("a-bike");
                    if let Some(v) = *CACHE.lock().expect("cache") {
                        return v;
                    }
                    BUILDS.fetch_add(1, Ordering::SeqCst);
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    *CACHE.lock().expect("cache") = Some(7);
                    7
                })
            })
            .collect();

        for c in callers {
            assert_eq!(c.join().expect("caller finished"), 7);
        }
        assert_eq!(BUILDS.load(Ordering::SeqCst), 1, "one build, three waiters");
    }

    #[test]
    fn different_keys_do_not_wait_on_each_other() {
        let held = enter("one");
        // Would block forever if the gate were global rather than per key.
        drop(enter("two"));
        drop(held);
    }

    #[test]
    fn a_failed_build_does_not_wedge_the_key() {
        drop(enter("wedged")); // as the `?` out of a build that errored would
        drop(enter("wedged"));
    }
}
