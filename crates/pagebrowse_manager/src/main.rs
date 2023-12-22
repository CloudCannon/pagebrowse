use std::io::BufRead;
use std::io::Write;
use std::sync::mpsc::Sender;

use base64::engine::general_purpose;
use base64::Engine;
use pagebrowse_manager::options::get_cli_matches;
use pagebrowse_manager::platforms;
use pagebrowse_manager::platforms::PBPlatform;
use pagebrowse_manager::PBRequest;
use pagebrowse_manager::PBRequestPayload;
use pagebrowse_manager::PBResponse;
use pagebrowse_manager::PBResponsePayload;
use tao::dpi::PhysicalPosition;
use tao::dpi::Position;
use tao::event_loop::EventLoopProxy;
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
    let message_id = msg.message_id.expect("Inbound requests have a message ID");

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
            item.assigned_to = Some(assigned_to);

            outgoing_tx
                .send(PBResponse {
                    message_id: Some(message_id),
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
                    message_id: Some(message_id),
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

            let x = (window_in_pool.assigned_to.unwrap() % 10) * 180;
            let y = (window_in_pool.assigned_to.unwrap() / 10) * 800;

            window_in_pool
                .window
                .set_outer_position(PhysicalPosition::new(x as u32, y as u32));

            outgoing_tx
                .send(PBResponse {
                    message_id: Some(message_id),
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
                    message_id: Some(message_id),
                    payload: PBResponsePayload::OperationComplete,
                })
                .expect("handle this error one day");
        }
        PBRequestPayload::Screenshot { window_id, path } => {
            let window_in_pool = pool
                .get_assigned_window(window_id)
                .expect("Consumer is behaving");

            let screenshot_callback = move |bytes: &[u8]| {
                eprintln!("Called back the bytes");
                let reader = image::io::Reader::new(std::io::Cursor::new(bytes))
                    .with_guessed_format()
                    .expect("Cursor io never fails");
                let image = reader.decode().unwrap();

                image.save(path.clone()).unwrap();

                outgoing_tx
                    .send(PBResponse {
                        message_id: Some(message_id),
                        payload: PBResponsePayload::OperationComplete,
                    })
                    .expect("handle this error one day");
            };

            platforms::Platform::screenshot(&window_in_pool.webview, screenshot_callback);
        }
    };
}
