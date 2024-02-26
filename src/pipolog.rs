use log::{info, warn};

pub(crate) struct Logger {
}

impl Logger {
    pub fn init() -> () {
        env_logger::init();
    }
    pub fn log(s: &str) -> () {
        info!("{}", s);
    }
}
