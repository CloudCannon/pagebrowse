pub use std::path::Path;

pub use webkit2gtk::WebContextExt;
pub use webkit2gtk::WebViewExt;

pub use tao::platform::unix::WindowExtUnix;
pub use wry::WebViewExtUnix;

pub struct LinuxPlatform {}

impl super::PlatformSetterUpper for LinuxPlatform {
    fn setup() -> EventLoop<Box<PBRequest>> {
        let event_loop = EventLoopBuilder::<Box<PBRequest>>::with_user_event().build();

        // TODO: Ability to hide from some kind of visibility

        event_loop
    }

    fn enhance_webview(webview: &wry::WebView) {
        webview
            .webview()
            .web_context()
            .unwrap()
            .set_web_extensions_directory("target/debug");
    }
}
