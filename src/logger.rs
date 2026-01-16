pub fn init_logger(verbosity: u8) {
    let level = match verbosity {
        0 => log::LevelFilter::Off,
        1 => log::LevelFilter::Info,
        2 => log::LevelFilter::Debug,
        3 => log::LevelFilter::Trace,
        _ => log::LevelFilter::Trace,
    };

    // `run_sendmail` can be invoked multiple times in-process (e.g. integration tests).
    // `env_logger::init()` panics if called more than once, so make this idempotent.
    let _ = env_logger::Builder::from_default_env()
        .filter_level(level)
        .format_timestamp(None)
        .format_target(false)
        .try_init();
}
