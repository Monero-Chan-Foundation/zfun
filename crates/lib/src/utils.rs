pub const CYCLE_TRACKER_ENABLED: bool = cfg!(all(
    feature = "cycle-tracker",
    target_os = "zkvm",
    target_vendor = "monerochan"
));

// Cycle tracking macro
#[macro_export]
macro_rules! track_cycles {
    ($e:expr, $($arg:tt)*) => {
        {
            if $crate::CYCLE_TRACKER_ENABLED {
                println!("cycle-tracker-start: {}", format!($($arg)*));
            }

            #[allow(clippy::let_and_return)]
            let __cycle_track_result = $e;

            if $crate::CYCLE_TRACKER_ENABLED {
                println!("cycle-tracker-end: {}", format!($($arg)*));
            }

            __cycle_track_result
        }
    };
}
