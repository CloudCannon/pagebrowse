pub use std::path::Path;

use gtk::gio::Cancellable;
use gtk::prelude::WidgetExt;
pub use webkit2gtk::WebContextExt;
pub use webkit2gtk::WebViewExt;

pub use tao::platform::unix::WindowExtUnix;
use tao::event_loop::{EventLoop, EventLoopBuilder};
pub use wry::WebViewExtUnix;

use javascriptcore::ValueExt;

use crate::PBEvent;

pub struct LinuxPlatform {}

impl super::PBPlatform for LinuxPlatform {
    fn setup() -> EventLoop<Box<PBEvent>> {
        
        gtk::init().unwrap();
        let event_loop = EventLoopBuilder::<Box<PBEvent>>::with_user_event().build();

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

    fn screenshot(webview: &wry::WebView, bytes_callback: impl Fn(&[u8]) -> ()) {
        // Linux screenshotting
        // if let Some(window) = webview.window().gtk_window().window() {
        //     let inner_size = webview.window().inner_size();
        //     window
        //         .pixbuf(0, 0, inner_size.width as i32, inner_size.height as i32)
        //         .unwrap()
        //         .savev(Path::new("/workspace/test.jpg"), "jpeg", &[]);
        // }
    }

    fn run_js(webview: &wry::WebView, js: &str, output_callback: impl Fn(String) -> () + 'static) {
        webview.webview().call_async_javascript_function(js, None, None, None, Cancellable::NONE, move |res| {
            let mut result = String::new();
            if let Ok(output) = res {
                if let Some(json) = output.to_json(0) {
                    result = json.to_string();
                }
            }
            output_callback(result);
        });
    }
}
