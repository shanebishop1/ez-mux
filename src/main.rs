#![cfg_attr(not(test), deny(clippy::unwrap_used))]

fn main() {
    std::process::exit(ez_mux::run());
}
