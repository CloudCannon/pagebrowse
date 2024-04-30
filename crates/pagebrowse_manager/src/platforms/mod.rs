#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub use macos::MacOSPlatform as Platform;

#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub use linux::LinuxPlatform as Platform;
use tao::event_loop::EventLoop;

use crate::PBEvent;

pub trait PBPlatform {
    fn setup() -> EventLoop<Box<PBEvent>>;
    fn enhance_webview(webview: &wry::WebView);
    fn screenshot(webview: &wry::WebView, bytes_callback: impl Fn(&[u8]) -> ());
    fn run_js(webview: &wry::WebView, js: &str, output_callback: impl Fn(String) -> () + 'static);
}
