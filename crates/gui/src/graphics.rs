//! The graphics backends the window starts with. Iced has wgpu start all of its backends and pick
//! the best adapter among them, OpenGL's (EGL) included. Where that pick is a Vulkan GPU anyway,
//! the window starts with Vulkan alone. A check after each start, with the backends already
//! loaded, keeps that so: a Vulkan that stopped working costs one start drawn in software (Iced's
//! fallback), then every backend is started again.

use std::ffi::OsStr;
use std::time::Duration;

use slatty_core::db::Db;
use wgpu::{AdapterInfo, Backend, Backends, DeviceType};

/// How long after the start the check waits: run at once, it slowed the first frame by 16 ms
/// (another Vulkan instance made beside Iced's), and its answer is only for the next start.
pub const CHECK_AFTER: Duration = Duration::from_secs(5);

/// The backends to start with: `None` when the user chose them (`WGPU_BACKEND`), left as they are.
pub fn backends(chosen: Option<&OsStr>, vulkan_only: bool) -> Option<Backends> {
    match (chosen, vulkan_only) {
        (Some(_), _) => None,
        (None, true) => Some(Backends::VULKAN),
        (None, false) => Some(Backends::all()),
    }
}

/// Whether wgpu picked a Vulkan GPU: not a software one (lavapipe), drawn by the processor.
pub fn vulkan_gpu(info: &AdapterInfo) -> bool {
    info.backend == Backend::Vulkan && info.device_type != DeviceType::Cpu
}

/// Asks wgpu, started with `backends` as the window was, for an adapter the way Iced does, and
/// keeps whether the next start can be Vulkan alone.
pub async fn check(db: &Db, backends: Backends) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends,
        ..Default::default()
    });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::from_env()
                .unwrap_or(wgpu::PowerPreference::HighPerformance),
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await;
    let vulkan = adapter.is_ok_and(|a| vulkan_gpu(&a.get_info()));
    if slatty_core::settings::vulkan_only(db).ok() != Some(vulkan) {
        let _ = slatty_core::settings::set_vulkan_only(db, vulkan);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vulkan alone once found to be what wgpu picks; the user's own choice stays theirs.
    #[test]
    fn vulkan_alone_once_it_is_what_wgpu_would_pick() {
        assert_eq!(backends(None, false), Some(Backends::all()));
        assert_eq!(backends(None, true), Some(Backends::VULKAN));
        assert_eq!(backends(Some(OsStr::new("gl")), true), None);

        let info = |backend, device_type| AdapterInfo {
            name: "[FAKE] GPU".into(),
            vendor: 0,
            device: 0,
            device_type,
            driver: String::new(),
            driver_info: String::new(),
            backend,
        };
        assert!(vulkan_gpu(&info(Backend::Vulkan, DeviceType::DiscreteGpu)));
        assert!(vulkan_gpu(&info(
            Backend::Vulkan,
            DeviceType::IntegratedGpu
        )));
        assert!(!vulkan_gpu(&info(Backend::Vulkan, DeviceType::Cpu)));
        assert!(!vulkan_gpu(&info(Backend::Gl, DeviceType::DiscreteGpu)));
    }
}
