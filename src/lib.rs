#![allow(clippy::missing_errors_doc)]

pub mod api;
pub mod auth;
pub mod cli;
pub mod commands;
pub mod config;
pub mod error;
pub mod model;
pub mod output;

pub use commands::run;
