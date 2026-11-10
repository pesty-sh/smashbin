//! The `smashbin` program.
//!
//! This file is the whole binary crate. The image work lives in
//! `smashbin-core`. A library crate has no `main`, so this file's only job
//! is to start that library and pass the exit code back to the shell.

fn main() -> std::process::ExitCode {
    smashbin_core::start()
}
