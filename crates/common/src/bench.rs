use indexmap::map::IndexMap;
use lazy_static::lazy_static;
use pad::PadStr;
use std::{
    fmt,
    sync::Mutex,
    time::{Duration, Instant},
};

lazy_static! {
    pub static ref BENCH: Mutex<Benches> = Mutex::new(Benches::new(cfg!(debug_assertions)));
}

#[macro_use]
mod bench_macro {
    #[macro_export]
    macro_rules! bench {
        ($name:expr, $bench:expr) => {{
            if cfg!(debug_assertions) {
                // Do some dancing with catch_unwind to prevent panicking while holding the lock
                let res = {
                    let mut bench = common::BENCH.lock().unwrap();
                    bench.bench($name, || {
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $bench))
                    })
                };
                res.unwrap()
            } else {
                $bench
            }
        }};
    }
}

pub struct Benches {
    benches: IndexMap<String, Bench>,
    enable: bool,
}

impl Benches {
    pub fn bench<T, F: FnOnce() -> T>(&mut self, bencher: &str, bench: F) -> T {
        if self.enable {
            match self.benches.get_mut(bencher) {
                Some(b) => b.bench(bench),
                None => {
                    self.benches
                        .insert(bencher.to_string(), Bench::new(bencher));
                    self.bench(bencher, bench)
                }
            }
        } else {
            bench()
        }
    }

    pub fn new(enable: bool) -> Self {
        Self {
            benches: IndexMap::with_capacity(5),
            enable,
        }
    }
}

impl fmt::Display for Benches {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for bench in self.benches.values() {
            writeln!(f, "{}", bench)?
        }
        Ok(())
    }
}

pub struct Bench {
    name: String,
    count: usize,
    total_time: Duration,
}

impl Bench {
    pub fn bench<T, F: FnOnce() -> T>(&mut self, bench: F) -> T {
        self.count += 1;
        let start = Instant::now();
        let res = bench();
        self.total_time += start.elapsed();
        res
    }

    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            count: 0,
            total_time: Duration::new(0, 0),
        }
    }
}

impl fmt::Display for Bench {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "bench {}: total {}ms over {} times",
            self.name.pad_to_width(15),
            self.total_time.as_millis(),
            self.count,
        )
    }
}
