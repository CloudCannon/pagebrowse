pub use std::path::Path;

pub use webkit2gtk::WebContextExt;
pub use webkit2gtk::WebViewExt;

pub use tao::platform::unix::WindowExtUnix;
pub use wry::WebViewExtUnix;

pub struct LinuxPlatform {}

impl super::PBPlatform for LinuxPlatform {
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

    fn screenshot(webview: &wry::WebView) {
        // Linux screenshotting
        if let Some(window) = webview.window().gtk_window().window() {
            let inner_size = webview.window().inner_size();
            window
                .pixbuf(0, 0, inner_size.width as i32, inner_size.height as i32)
                .unwrap()
                .savev(Path::new("/workspace/test.jpg"), "jpeg", &[]);
        }
    }
}
