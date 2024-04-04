pub use std::{ffi::c_char, slice, str};

pub use block::ConcreteBlock;
pub use cocoa::base::{id, NO, YES};
use cocoa::{base::nil, foundation::NSArray};
pub use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel, BOOL},
    sel, sel_impl,
};

use tao::{
    event_loop::{EventLoop, EventLoopBuilder},
    platform::macos::EventLoopExtMacOS,
};
pub use wry::WebViewExtMacOS;

use crate::PBEvent;

pub struct MacOSPlatform {}

impl super::PBPlatform for MacOSPlatform {
    fn setup() -> EventLoop<Box<PBEvent>> {
        setup_macos();

        let mut event_loop = EventLoopBuilder::<Box<PBEvent>>::with_user_event().build();

        event_loop.set_activation_policy(tao::platform::macos::ActivationPolicy::Accessory);

        event_loop
    }

    fn enhance_webview(_webview: &wry::WebView) {
        /* no-op */
    }

    fn screenshot(webview: &wry::WebView, bytes_callback: impl Fn(&[u8]) -> ()) {
        // MacOS screenshotting
        unsafe {
            let webview: id = webview.webview();
            let block = ConcreteBlock::new(move |image: id, _error: id| {
                let image_data: id = msg_send![image, TIFFRepresentation];
                let image_rep: id = msg_send![class!(NSBitmapImageRep), alloc];
                let image_rep: id = msg_send![image_rep, initWithData:image_data];
                let image_data: id = msg_send![image_rep, representationUsingType:4 properties:nil];

                let len = msg_send![image_data, length];
                let byte_ptr: *const u8 = msg_send![image_data, bytes];
                let bytes = slice::from_raw_parts(byte_ptr, len);

                bytes_callback(bytes);
            });
            let conf: id = msg_send![class!(WKSnapshotConfiguration), alloc];
            let conf: id = msg_send![conf, init];
            let _: () =
                msg_send![webview, takeSnapshotWithConfiguration: conf completionHandler: block];
        }
    }

    fn run_js(webview: &wry::WebView, js: &str, output_callback: impl Fn(String) -> ()) {
        // MacOS javascript evaluation
        unsafe {
            let webview: id = webview.webview();

            let block = ConcreteBlock::new(move |output_value: id, _error: id| {
                let mut result = String::new();

                if output_value != nil {
                    let serializer = class!(NSJSONSerialization);
                    let json_ns_data: NSData = msg_send![serializer, dataWithJSONObject:output_value options:NS_JSON_WRITING_FRAGMENTS_ALLOWED error:nil];
                    let json_string = NSString::from(json_ns_data);

                    result = json_string.to_str().to_string();
                }

                output_callback(result)
            });

            let content_world: id = msg_send![class!(WKContentWorld), defaultClientWorld];

            let args: id = msg_send![class!(NSDictionary), dictionary];

            let _: () = msg_send![webview, callAsyncJavaScript:NSString::new(js) arguments:args inFrame:nil inContentWorld:content_world completionHandler:block];
        }
    }
}

pub fn setup_macos() {
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

//Also borrowed from Wry:
const NS_JSON_WRITING_FRAGMENTS_ALLOWED: u64 = 4;

//NSString implementation borrowed from Wry
const UTF8_ENCODING: usize = 4;
struct NSString(id);

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

impl From<NSData> for NSString {
    fn from(value: NSData) -> Self {
        Self(unsafe {
            let ns_string: id = msg_send![class!(NSString), alloc];
            let ns_string: id = msg_send![ns_string, initWithData:value encoding:UTF8_ENCODING];
            let _: () = msg_send![ns_string, autorelease];

            ns_string
        })
    }
}

//NSData implementation borrowed from Wry
struct NSData(id);
