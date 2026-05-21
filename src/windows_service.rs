/*
 * Copyright (C) 2026 VeryBaaad <verybaaad@outlook.com>
 * SPDX-License-Identifier: MIT
 */

#![cfg(target_os = "windows")]

use crate::utils::logger::log;
use log::Level;
use std::ffi::OsString;
use std::sync::{Arc, Mutex};
use tokio::runtime::Builder;
use tokio::sync::watch;
use windows_service::define_windows_service;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::ServiceStatusHandle;
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_dispatcher;

const SERVICE_NAME: &str = "zw-rs";

define_windows_service!(ffi_service_main, service_main);

pub fn run() -> anyhow::Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
    Ok(())
}

pub fn try_run_as_service() -> anyhow::Result<bool> {
    match run() {
        Ok(()) => Ok(true),
        Err(err) => {
            // ERROR_FAILED_SERVICE_CONTROLLER_CONNECT (1063):
            // not launched by the Windows Service Control Manager.
            if is_scm_connect_error(&err) {
                return Ok(false);
            }
            Err(err)
        }
    }
}

fn is_scm_connect_error(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .and_then(std::io::Error::raw_os_error)
            == Some(1063)
    })
}

fn service_main(_arguments: Vec<OsString>) {
    if let Err(err) = run_service() {
        log(
            Level::Error,
            "ZWBotDaemon",
            &format!("Windows service exited with error: {err:#}"),
        );
    }
}

fn run_service() -> anyhow::Result<()> {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let shutdown_tx = Arc::new(shutdown_tx);
    let status_handle_slot: Arc<Mutex<Option<ServiceStatusHandle>>> = Arc::new(Mutex::new(None));
    let status_handle_slot_for_handler = status_handle_slot.clone();

    let status_handle =
        service_control_handler::register(SERVICE_NAME, move |control| match control {
            ServiceControl::Stop => {
                if let Ok(guard) = status_handle_slot_for_handler.lock()
                    && let Some(handle) = guard.as_ref()
                {
                    let _ = handle.set_service_status(ServiceStatus {
                        service_type: ServiceType::OWN_PROCESS,
                        current_state: ServiceState::StopPending,
                        controls_accepted: ServiceControlAccept::empty(),
                        exit_code: ServiceExitCode::Win32(0),
                        checkpoint: 1,
                        wait_hint: std::time::Duration::from_secs(30),
                        process_id: None,
                    });
                }
                let _ = shutdown_tx.send(true);
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })?;
    if let Ok(mut guard) = status_handle_slot.lock() {
        *guard = Some(status_handle.clone());
    }

    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StartPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: std::time::Duration::from_secs(10),
        process_id: None,
    })?;

    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: std::time::Duration::default(),
        process_id: None,
    })?;

    let rt = Builder::new_multi_thread().enable_all().build()?;
    let service_result = rt.block_on(crate::run_bot(false, Some(shutdown_rx)));

    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: std::time::Duration::default(),
        process_id: None,
    })?;

    service_result
}
