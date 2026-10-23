//! `libresvip-cli` — Rust port of the LibreSVIP command line interface.

mod commands;
mod mcp;
mod output;
mod prompt;
#[cfg(feature = "rpc")]
mod rpc;
mod translator;

fn main() {
    #[cfg(target_os = "windows")]
    // Match `libresvip.cli.__init__`, which switches the console to UTF-8.
    unsafe {
        #[link(name = "kernel32")]
        extern "system" {
            fn SetConsoleOutputCP(code_page: u32) -> i32;
            fn SetConsoleCP(code_page: u32) -> i32;
        }
        SetConsoleOutputCP(65001);
        SetConsoleCP(65001);
    }

    std::process::exit(commands::run());
}
