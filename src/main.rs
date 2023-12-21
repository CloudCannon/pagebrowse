#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
use std::path::Path;
#[cfg(target_os = "macos")]
use std::{ffi::c_char, slice, str};
use tao::event::Event;
use tao::event::StartCause;
use tao::event::WindowEvent;
use tao::event_loop::ControlFlow;
use tao::event_loop::EventLoop;
use tao::window::WindowBuilder;
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
use webkit2gtk::WebContextExt;
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
use webkit2gtk::WebViewExt;
use wry::WebViewBuilder;

#[cfg(target_os = "macos")]
use block::ConcreteBlock;
#[cfg(target_os = "macos")]
use cocoa::base::{id, NO, YES};
#[cfg(target_os = "macos")]
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel, BOOL},
    sel, sel_impl,
};
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
use tao::platform::unix::WindowExtUnix;
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
use wry::WebViewExtUnix;

use wry::raw_window_handle::HasRawWindowHandle;
#[cfg(target_os = "macos")]
use wry::WebViewExtMacOS;

#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
fn setup() {}
#[cfg(target_os = "macos")]
fn setup() {
    //====== NSURLProtocol Methods =========

    extern "C" fn can_init_with_request(_this: &Class, _: Sel, request: id) -> BOOL {
        //TODO: Add logic for tracking requests here

        //TODO: Check if request interception is enabled and exit early if not

        unsafe {
            let tagged: id = msg_send![class!(NSURLProtocol), propertyForKey:NSString::new("PageBrowseHandled") inRequest:request];
            let tagged = NSString(tagged);
            let tagged = tagged.to_str();
            if tagged == "PageBrowseHandled" {
                return NO;
            }
        }
        YES
    }

    extern "C" fn canonical_request_for_request(_this: &Class, _: Sel, request: id) -> id {
        //TODO: Maybe add logic for modifying the request here
        request
    }

    extern "C" fn start_loading(this: &mut Object, _: Sel) {
        unsafe {
            let request: &id = this.get_ivar::<id>("request");
            let new_request: id = msg_send![*request, mutableCopy];

            let _: () = msg_send![
            class!(NSURLProtocol),
            setProperty:NSString::new("PageBrowseHandled")
            forKey:NSString::new("PageBrowseHandled")
            inRequest:new_request
            ];

            let new_this: *mut Object = this;

            //TODO: Maybe add logic for modifying the request here
            //TODO: Add logic for stubbing (i.e blocking) the request here

            let connection: id = msg_send![class!(NSURLConnection), connectionWithRequest:new_request delegate:new_this];

            this.set_ivar("connection", connection);
        };
    }

    extern "C" fn stop_loading(_this: &Object, _: Sel) {
        //TODO: Maybe add some logic here for cleaning up the connection
    }

    extern "C" fn init_with_request(
        this: &mut Object,
        _: Sel,
        request: id,
        _cached_response: id,
        client: id,
    ) -> id {
        unsafe { this.set_ivar("client", client) };
        unsafe { this.set_ivar("request", request) };
        let this: *mut Object = this;
        this
    }

    //====== Connection Delegate Methods =========

    extern "C" fn did_receive_response(this: &Object, _: Sel, _connection: id, response: id) {
        unsafe {
            let client: &id = this.get_ivar::<id>("client");
            // I think cachePolicy:3 is never cache, but might be worth investigating
            let _: () = msg_send![*client, URLProtocol:this didReceiveResponse: response cacheStoragePolicy:3];
        }
    }

    extern "C" fn did_receive_data(this: &Object, _: Sel, _connection: id, data: id) {
        unsafe {
            let client: &id = this.get_ivar::<id>("client");
            let _: () = msg_send![*client, URLProtocol:this didLoadData: data];
        }
    }

    extern "C" fn did_finish_loading(this: &Object, _: Sel, _connection: id) {
        unsafe {
            let client: &id = this.get_ivar::<id>("client");
            let _: () = msg_send![*client, URLProtocolDidFinishLoading: this];
        }
    }

    //Unspeakable black magic to enable request interception for http(s)
    unsafe {
        let cls = class!(WKBrowsingContextController);
        let sel = sel!(registerSchemeForCustomProtocol:);
        let _: () = msg_send![cls, performSelector:sel withObject:NSString::new("http")];
        let _: () = msg_send![cls, performSelector:sel withObject:NSString::new("https")];
    }

    //Create request interception class in Obj-C
    let mut cls = ClassDecl::new("InterceptProtocol", class!(NSURLProtocol)).unwrap();
    unsafe {
        cls.add_ivar::<id>("client");
        cls.add_ivar::<id>("request");
        cls.add_ivar::<id>("connection");
        cls.add_class_method(
            sel!(canInitWithRequest:),
            can_init_with_request as extern "C" fn(&Class, Sel, id) -> BOOL,
        );
        cls.add_class_method(
            sel!(canonicalRequestForRequest:),
            canonical_request_for_request as extern "C" fn(&Class, Sel, id) -> id,
        );
        cls.add_method(
            sel!(startLoading),
            start_loading as extern "C" fn(&mut Object, Sel),
        );
        cls.add_method(
            sel!(stopLoading),
            stop_loading as extern "C" fn(&Object, Sel),
        );
        cls.add_method(
            sel!(initWithRequest:cachedResponse:client:),
            init_with_request as extern "C" fn(&mut Object, Sel, id, id, id) -> id,
        );
        cls.add_method(
            sel!(connection:didReceiveResponse:),
            did_receive_response as extern "C" fn(&Object, Sel, id, id),
        );
        cls.add_method(
            sel!(connection:didReceiveData:),
            did_receive_data as extern "C" fn(&Object, Sel, id, id),
        );
        cls.add_method(
            sel!(connectionDidFinishLoading:),
            did_finish_loading as extern "C" fn(&Object, Sel, id),
        );
    }
    let cls = cls.register();

    unsafe {
        let _: () = msg_send![class!(NSURLProtocol), registerClass: cls];
    }
}

fn main() {
    setup();

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_visible(true)
        .build(&event_loop)
        .expect("Window should be created");

    let webview = WebViewBuilder::new(&window)
        //TODO: Add config options for allowing/preventing page navigation
        .with_navigation_handler(|url| {
            println!("Navigating to {url}");
            true
        })
        .build()
        .expect("Webview should create successfully");

    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    webview
        .webview()
        .web_context()
        .unwrap()
        .set_web_extensions_directory("crates/gtk-plugin/target/debug");

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        //MacOS screenshotting
        //TODO: Move into a public method for screenshotting the webview
        // unsafe {
        // let webview: id = webview.webview();
        // let block = ConcreteBlock::new(|image: id, _error: id| {
        // let _image_data: id = msg_send![image, TIFFRepresentation];
        // TODO: Somehow return the image data as bytes, probably taking inspiration
        // from NSString::to_str()

        // TODO: Support other image formats
        // https://developer.apple.com/documentation/appkit/nsbitmapimagerep/1395458-representation
        // https://developer.apple.com/forums/thread/66779
        // });
        // let conf: id = msg_send![class!(WKSnapshotConfiguration), alloc];
        // let conf: id = msg_send![conf, init];
        // let _: () =
        //     msg_send![webview, takeSnapshotWithConfiguration: conf completionHandler: block];
        // }

        // Linux screenshotting
        // if let Some(window) = webview.window().gtk_window().window() {
        //     let inner_size = webview.window().inner_size();
        //     window
        //         .pixbuf(0, 0, inner_size.width as i32, inner_size.height as i32)
        //         .unwrap()
        //         .savev(Path::new("/workspace/test.jpg"), "jpeg", &[]);
        // }

        match event {
            Event::NewEvents(StartCause::Init) => {
                println!("Wry has started!");

                //TODO: Move into a public method for navigating the webview
                webview.load_url("https://cloudcannon.com");

                //TODO: Move into a public method for evaluating javascript
                webview
                    .evaluate_script("document.body.prepend('🦀')")
                    .expect("Failed to eval script");

                //TODO: Move into a public method for resizing the webview
                //TODO: wry 0.35.0 — Webview::inner_size is removed
                // webview
                //     .window()
                //     .set_inner_size(PhysicalSize::new(1024, 720));
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            _ => (),
        }
    });
}

//NSString implementation borrowed from Wry
#[cfg(target_os = "macos")]
const UTF8_ENCODING: usize = 4;
#[cfg(target_os = "macos")]

struct NSString(id);

#[cfg(target_os = "macos")]
impl NSString {
    fn new(s: &str) -> Self {
        // Safety: objc runtime calls are unsafe
        NSString(unsafe {
            let ns_string: id = msg_send![class!(NSString), alloc];
            let ns_string: id = msg_send![ns_string,
                            initWithBytes:s.as_ptr()
                            length:s.len()
                            encoding:UTF8_ENCODING];

            // The thing is allocated in rust, the thing must be set to autorelease in rust to relinquish control
            // or it can not be released correctly in OC runtime
            let _: () = msg_send![ns_string, autorelease];

            ns_string
        })
    }

    fn to_str(&self) -> &str {
        unsafe {
            let bytes: *const c_char = msg_send![self.0, UTF8String];
            let len = msg_send![self.0, lengthOfBytesUsingEncoding: UTF8_ENCODING];
            let bytes = slice::from_raw_parts(bytes as *const u8, len);
            str::from_utf8_unchecked(bytes)
        }
    }

    fn as_ptr(&self) -> id {
        self.0
    }
}
