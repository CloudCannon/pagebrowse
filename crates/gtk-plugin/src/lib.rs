use webkit2gtk_webextension::prelude::*;
use webkit2gtk_webextension::{WebExtension, WebPage};
use webkit2gtk_webextension_sys::{WebKitWebExtension, WebKitWebPage};

#[no_mangle]
#[doc(hidden)]
pub unsafe fn webkit_web_extension_initialize(extension: *mut WebKitWebExtension) {
    let extension: WebExtension = glib::translate::from_glib_none(extension);
    extension.connect_page_created(web_page_created_callback);
}

pub fn web_page_created_callback(extension: &WebExtension, web_page: &WebPage) {
    web_page.connect_send_request(|web_page, request, response| {
        println!("{:?}", request.uri());
        false
    });
}
