//! Solver work run apart from the games: in a rayon pool of its own, one
//! piece at a time.
//!
//! The solver builds its equity tables lazily, in a `OnceLock`, with rayon
//! tasks. A rayon thread that waits for tasks runs other tasks of its pool
//! meanwhile: built in the pool that plays the games, a table could have its
//! builder take up a game that waits on that very table, and the simulation
//! would hang. Run here, the solver's tasks never meet a game, and a game
//! thread waits for them without taking up anything else.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Runs `work` in the solver's pool once no other work runs there, and
/// says how long it took once its turn came.
pub(crate) fn run<R: Send>(work: impl FnOnce() -> R + Send) -> (R, Duration) {
    static POOL: OnceLock<rayon::ThreadPool> = OnceLock::new();
    // One at a time, for a piece of work can itself fill a table lazily.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
    let pool = POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .thread_name(|i| format!("solver-{i}"))
            .build()
            .expect("the solver's thread pool starts")
    });
    // A plain thread waits without running games, unlike a rayon one.
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                let _turn = ONE_AT_A_TIME
                    .lock()
                    .expect("no thread panics while holding its turn");
                let start = Instant::now();
                let result = pool.install(work);
                (result, start.elapsed())
            })
            .join()
            .expect("the solver does not panic")
    })
}
