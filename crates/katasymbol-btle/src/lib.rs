//! [`Link`] over [`btleplug`]: CoreBluetooth on macOS, WinRT on Windows.
//!
//! (On Linux btleplug goes through BlueZ's D-Bus API, which connects the
//! dual-mode E16 over BR/EDR — use `katasymbol-linux` there.)
//!
//! Must run inside a Tokio runtime (`rt` + `time`).

use btleplug::api::{Central, Characteristic, Manager as _, Peripheral as _, ScanFilter, ValueNotification, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures::{Stream, StreamExt};
use katasymbol_core::{Error, Link, Result};
use std::pin::Pin;
use std::time::Duration;
use uuid::Uuid;

/// The Supvan service and its characteristics (Bluetooth-base UUIDs).
pub const SERVICE: Uuid = Uuid::from_u128(0x0000e0ff_3c17_d293_8e48_14fe2e4da212);
pub const NOTIFY: Uuid = Uuid::from_u128(0x0000ffe1_0000_1000_8000_00805f9b34fb);
pub const WRITE: Uuid = Uuid::from_u128(0x0000ffe9_0000_1000_8000_00805f9b34fb);

fn err(e: impl std::fmt::Display) -> Error {
    Error::Link(e.to_string())
}

/// A nearby device seen during a scan.
#[derive(Debug, Clone)]
pub struct Seen {
    /// Platform identifier (a UUID on macOS, the address on Windows).
    pub id: String,
    pub name: String,
}

async fn adapter() -> Result<Adapter> {
    let manager = Manager::new().await.map_err(err)?;
    manager
        .adapters()
        .await
        .map_err(err)?
        .into_iter()
        .next()
        .ok_or_else(|| Error::Link("kein Bluetooth-Adapter gefunden".into()))
}

/// Scan for `secs` seconds and list named devices.
pub async fn scan(secs: u64) -> Result<Vec<Seen>> {
    let adapter = adapter().await?;
    adapter.start_scan(ScanFilter::default()).await.map_err(err)?;
    tokio::time::sleep(Duration::from_secs(secs)).await;
    let mut seen = Vec::new();
    for p in adapter.peripherals().await.map_err(err)? {
        if let Ok(Some(props)) = p.properties().await
            && let Some(name) = props.local_name
        {
            seen.push(Seen { id: p.id().to_string(), name });
        }
    }
    let _ = adapter.stop_scan().await;
    Ok(seen)
}

/// A connected printer.
pub struct BtleLink {
    peripheral: Peripheral,
    write: Characteristic,
    notifications: Pin<Box<dyn Stream<Item = ValueNotification> + Send>>,
}

impl BtleLink {
    /// Connect to the device with identifier `id` (from [`scan`]). When the
    /// device is not known yet, scan briefly for it first.
    pub async fn connect(id: &str) -> Result<Self> {
        let adapter = adapter().await?;
        let find = |ps: Vec<Peripheral>| ps.into_iter().find(|p| p.id().to_string() == id);
        let mut peripheral = find(adapter.peripherals().await.map_err(err)?);
        if peripheral.is_none() {
            adapter.start_scan(ScanFilter::default()).await.map_err(err)?;
            for _ in 0..40 {
                tokio::time::sleep(Duration::from_millis(250)).await;
                peripheral = find(adapter.peripherals().await.map_err(err)?);
                if peripheral.is_some() {
                    break;
                }
            }
            let _ = adapter.stop_scan().await;
        }
        let peripheral = peripheral.ok_or_else(|| Error::Link(format!("Drucker {id} nicht gefunden – ist er an?")))?;
        peripheral.connect().await.map_err(err)?;
        peripheral.discover_services().await.map_err(err)?;
        let chars = peripheral.characteristics();
        let pick = |u: Uuid| chars.iter().find(|c| c.uuid == u && c.service_uuid == SERVICE).cloned();
        let (notify, write) = match (pick(NOTIFY), pick(WRITE)) {
            (Some(n), Some(w)) => (n, w),
            _ => return Err(Error::Link("Drucker hat keine ffe1/ffe9-Kanäle".into())),
        };
        peripheral.subscribe(&notify).await.map_err(err)?;
        let notifications = peripheral.notifications().await.map_err(err)?;
        Ok(Self { peripheral, write, notifications })
    }

    pub async fn disconnect(&self) {
        let _ = self.peripheral.disconnect().await;
    }
}

impl Link for BtleLink {
    async fn write(&mut self, chunk: &[u8]) -> Result<()> {
        self.peripheral.write(&self.write, chunk, WriteType::WithoutResponse).await.map_err(err)
    }

    async fn recv(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            match tokio::time::timeout_at(deadline, self.notifications.next()).await {
                Ok(Some(n)) if n.uuid == NOTIFY => return Ok(Some(n.value)),
                Ok(Some(_)) => continue,
                Ok(None) => return Err(Error::Link("printer disconnected".into())),
                Err(_) => return Ok(None),
            }
        }
    }

    async fn sleep(&mut self, d: Duration) {
        tokio::time::sleep(d).await;
    }
}
