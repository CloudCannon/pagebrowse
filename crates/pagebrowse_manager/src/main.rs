use std::collections::HashMap;
use std::io::BufRead;
use std::io::Write;
use std::sync::mpsc::Sender;
use std::time::Duration;

use base64::engine::general_purpose;
use base64::Engine;
use pagebrowse_manager::options::get_cli_matches;
use pagebrowse_manager::platforms;
use pagebrowse_manager::platforms::PlatformSetterUpper;
use pagebrowse_manager::PBRequest;
use pagebrowse_manager::PBRequestPayload;
use pagebrowse_manager::PBResponse;
use pagebrowse_manager::PBResponsePayload;
use tao::event_loop::EventLoopBuilder;
use tao::event_loop::EventLoopProxy;
use tao::platform::macos::EventLoopExtMacOS;
use tao::window::Window;
use wry::WebView;

use tao::dpi::PhysicalSize;
use tao::event::Event;
use tao::event::StartCause;
use tao::event::WindowEvent;
use tao::event_loop::ControlFlow;
use tao::event_loop::EventLoop;
use tao::window::WindowBuilder;

use wry::WebViewBuilder;

struct PoolItem {
    window: Window,
    webview: WebView,
    assigned_to: Option<u32>,
}

struct WindowReference {
    is_active: bool,
    pool_index: usize,
}

struct Pool {
    items: Vec<PoolItem>,
    assignments: Vec<WindowReference>,
}

impl Pool {
    fn new(count: usize, visible: bool, event_loop: &EventLoop<Box<PBRequest>>) -> Self {
        let pool_items: Vec<PoolItem> = (0..count)
            .map(|i| {
                let window = WindowBuilder::new()
                    .with_visible(visible)
                    .build(&event_loop)
                    .expect("Window should be created");

                let webview = WebViewBuilder::new(&window)
                    //TODO: Add config options for allowing/preventing page navigation
                    .with_navigation_handler(move |url| {
                        eprintln!("Webview {i} is navigating to {url}");
                        true
                    })
                    .build()
                    .expect("Webview should create successfully");

                platforms::Platform::enhance_webview(&webview);

                PoolItem {
                    window,
                    webview,
                    assigned_to: None,
                }
            })
            .collect();

        Self {
            items: pool_items,
            assignments: vec![],
        }
    }

    fn get_assigned_window(&self, window_id: u32) -> Result<&PoolItem, ()> {
        let window_assignment = self
            .assignments
            .get(window_id as usize)
            .expect("Add error type for future ids");

        if !window_assignment.is_active {
            unimplemented!("Add error type for reusing dead windows");
        }

        let window_in_pool = self.items.get(window_assignment.pool_index).unwrap();

        Ok(window_in_pool)
    }
}

fn start_listening(proxy: EventLoopProxy<Box<PBRequest>>, outgoing_tx: Sender<PBResponse>) {
    std::thread::spawn(move || {
        /*
           STDIN comms
        */
        let event_loop_proxy = proxy;
        let mut stdin = std::io::stdin().lock();

        loop {
            let mut buf = vec![];
            stdin.read_until(b',', &mut buf).unwrap();

            if buf.pop().is_none() {
                // EOF Reached
                std::process::exit(0);
            }

            let Ok(decoded) = general_purpose::STANDARD.decode(buf) else {
                outgoing_tx
                    .send(PBResponse {
                        message_id: None,
                        payload: PBResponsePayload::Error {
                            original_message: None,
                            message: "Unparseable message, not valid base64".into(),
                        },
                    })
                    .expect("Channel is open");
                return;
            };

            match serde_json::from_slice::<PBRequest>(&decoded) {
                Ok(msg) => {
                    _ = event_loop_proxy.send_event(Box::new(msg));
                }
                Err(e) => {
                    let error = match std::str::from_utf8(&decoded[..]) {
                        Ok(msg) => PBResponsePayload::Error {
                            original_message: Some(msg.to_string()),
                            message: format!("{e}"),
                        },
                        Err(_) => PBResponsePayload::Error {
                            original_message: None,
                            message: "Pagefind was unable to parse the message it was provided via the service".to_string(),
                        },
                    };

                    outgoing_tx
                        .send(PBResponse {
                            message_id: None,
                            payload: error,
                        })
                        .expect("Channel is open");
                }
            }
        }
    });
}

fn main() {
    let (outgoing_tx, mut outgoing_rx) = std::sync::mpsc::channel::<PBResponse>();

    let options = get_cli_matches();
    let windows_are_visible = options.get_flag("visible");
    let window_count = options
        .get_one::<usize>("count")
        .expect("window count is required");

    let event_loop = platforms::Platform::setup();
    let proxy = event_loop.create_proxy();

    let mut pool = Pool::new(*window_count, windows_are_visible, &event_loop);

    std::thread::spawn(move || {
        let mut stdout = std::io::stdout().lock();

        loop {
            let msg = outgoing_rx.recv().unwrap();
            let encoded = general_purpose::STANDARD.encode(serde_json::to_vec(&msg).unwrap());

            stdout.write_all(encoded.as_bytes()).unwrap();
            stdout.write(b",").unwrap();
            stdout.flush().unwrap();
        }
    });

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::UserEvent(msg) => {
                handle_message(*msg, &mut pool, outgoing_tx.clone());
            }
            Event::NewEvents(StartCause::Init) => {
                eprintln!("Wry has started!");

                start_listening(proxy.clone(), outgoing_tx.clone());

                // pool.items.iter().for_each(|item| {
                //     //TODO: Move into a public method for navigating the webview
                //     item.webview.load_url("https://cloudcannon.com");

                //     //TODO: Move into a public method for evaluating javascript
                //     item.webview
                //         .evaluate_script("document.body.prepend('🦀')")
                //         .expect("Failed to eval script");

                //     //TODO: Move into a public method for resizing the webview
                //     item.window.set_inner_size(PhysicalSize::new(500, 1000));
                // });
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            _ => (),
        }
    });
}

fn handle_message(msg: PBRequest, pool: &mut Pool, outgoing_tx: Sender<PBResponse>) {
    eprintln!("===\nHandling the message: {msg:#?}\n===");
    let message_id = msg.message_id;

    match msg.payload {
        PBRequestPayload::Tester(string) => {
            outgoing_tx
                .send(PBResponse {
                    message_id: msg.message_id,
                    payload: PBResponsePayload::Tester(format!("Responding to [{string}]")),
                })
                .expect("Sendable");
        }
        PBRequestPayload::NewWindow { start_url } => {
            let Some((pool_index, item)) = pool
                .items
                .iter_mut()
                .enumerate()
                .find(|(_, item)| item.assigned_to.is_none())
            else {
                unimplemented!("Handle the window queue")
            };

            let assigned_to = pool.assignments.len() as u32;
            pool.assignments.push(WindowReference {
                is_active: true,
                pool_index,
            });

            outgoing_tx
                .send(PBResponse {
                    message_id,
                    payload: PBResponsePayload::NewWindowCreated { id: assigned_to },
                })
                .expect("handle this error one day");
        }
        PBRequestPayload::Navigate { window_id, url } => {
            let window_in_pool = pool
                .get_assigned_window(window_id)
                .expect("Consumer is behaving");

            window_in_pool.webview.load_url(&url);

            // TODO: Wait for it to actually navigate
            // DOMContentLoaded?

            outgoing_tx
                .send(PBResponse {
                    message_id,
                    payload: PBResponsePayload::OperationComplete,
                })
                .expect("handle this error one day");
        }
        PBRequestPayload::ResizeWindow {
            window_id,
            width,
            height,
        } => {
            let window_in_pool = pool
                .get_assigned_window(window_id)
                .expect("Consumer is behaving");

            window_in_pool
                .window
                .set_inner_size(PhysicalSize::new(width as u32, height as u32));

            outgoing_tx
                .send(PBResponse {
                    message_id,
                    payload: PBResponsePayload::OperationComplete,
                })
                .expect("handle this error one day");
        }
        PBRequestPayload::EvaluateScript { window_id, script } => {
            let window_in_pool = pool
                .get_assigned_window(window_id)
                .expect("Consumer is behaving");

            match window_in_pool.webview.evaluate_script(&script) {
                Ok(()) => {}
                Err(e) => eprintln!("{e}"),
            }

            outgoing_tx
                .send(PBResponse {
                    message_id,
                    payload: PBResponsePayload::OperationComplete,
                })
                .expect("handle this error one day");
        }
    };
}

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
