//! Kuberve version — single source of truth.
//! Меняется в `kernel/Cargo.toml` (`[package] version = ...`).
//! Все `*_STR` читают `CARGO_PKG_VERSION` через `env!` на этапе компиляции,
//! `concat!` склеивает с префиксом без рантайм-оверхеда.

pub const STR: &str = env!("CARGO_PKG_VERSION");
pub const OS_VERSION: &str = concat!("Kuberve v", env!("CARGO_PKG_VERSION"));
pub const CONSOLE_TITLE: &str = concat!("Kuberve Console v", env!("CARGO_PKG_VERSION"));
pub const KERNEL_NAME: &str = concat!("kuberve-", env!("CARGO_PKG_VERSION"), " aarch64");