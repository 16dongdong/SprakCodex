#![allow(dead_code)]

pub mod account;
pub mod diagnostics;
#[allow(non_snake_case)]
pub mod directObservation;
pub mod login;
mod registry;
pub mod requestlog;
pub mod service;
#[allow(non_snake_case)]
pub mod sessionRouting;
pub mod settings;
pub mod shared;
pub mod startup;
pub mod system;
pub mod updater;
pub mod usage;

pub(crate) use registry::invoke_handler;
