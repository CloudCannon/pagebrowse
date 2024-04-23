extern crate gtk;

use std::collections::HashMap;
use std::collections::VecDeque;
use std::io::BufRead;
use std::io::Write;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::sync::Mutex;

use base64::engine::general_purpose;
use base64::Engine;
use pagebrowse_manager::options::get_cli_matches;
use pagebrowse_manager::platforms;
use pagebrowse_manager::platforms::PBPlatform;
use pagebrowse_manager::InitializationParams;
use pagebrowse_manager::PBEvent;
use pagebrowse_manager::PBHook;
use pagebrowse_manager::PBRequest;
use pagebrowse_manager::PBRequestPayload;
use pagebrowse_manager::PBResponse;
use pagebrowse_manager::PBResponsePayload;
use pagebrowse_manager::PBWebviewEvent;
use tao::dpi::PhysicalPosition;
use tao::dpi::Position;
use tao::event_loop::EventLoopProxy;
use tao::platform::unix::WindowExtUnix;
use tao::window::Window;
use wry::PageLoadEvent;
use wry::WebView;

use tao::dpi::PhysicalSize;
use tao::event::Event;
use tao::event::StartCause;
use tao::event::WindowEvent;
use tao::event_loop::ControlFlow;
use tao::event_loop::EventLoop;
use tao::window::WindowBuilder;

use wry::WebViewBuilder;
use wry::WebViewBuilderExtUnix;

enum PoolEvent {
    PageLoad { inner: PageLoadEvent, url: String },
}

struct PoolItem {
    id: usize,
    window: Window,
    webview: WebView,
    assigned_to: Option<u32>,
    pending_responses: HashMap<PBWebviewEvent, PBResponse>,
}

struct WindowReference {
    pool_index: usize,
}

struct Pool {
    items: Vec<PoolItem>,
    assignments: HashMap<u32, WindowReference>,
    next_assignment: u32,
    waiting_for_windows: VecDeque<u32>,
}

impl Pool {
    fn new(
        params: InitializationParams,
        event_loop: &EventLoop<Box<PBEvent>>,
        proxy: EventLoopProxy<Box<PBEvent>>,
    ) -> Self {
        let pool_items: Vec<PoolItem> = (0..params.pool_size)
            .map(|i| {
                let window = WindowBuilder::new()
                    .with_visible(params.visible)
                    .build(&event_loop)
                    .expect("Window should be created");

                // TODO: Add .with_on_page_load_handler(handler) to the below
                let this_proxy = proxy.clone();

                #[cfg(target_os = "macos")]
                let mut builder = WebViewBuilder::new(&window)
                    .with_navigation_handler(move |url| {
                        // eprintln!("Webview {i} is navigating to {url}");
                        true
                    })
                    .with_on_page_load_handler(move |inner, url| {
                        let hook = match inner {
                            PageLoadEvent::Started => PBHook {
                                pool_item: i,
                                event: PBWebviewEvent::PageLoadStart { url },
                            },
                            PageLoadEvent::Finished => PBHook {
                                pool_item: i,
                                event: PBWebviewEvent::PageLoadFinish { url },
                            },
                        };

                        if this_proxy
                            .send_event(Box::new(PBEvent::Hook(hook)))
                            .is_err()
                        {
                            panic!("todo");
                        };
                    });

                #[cfg(any(
                    target_os = "linux",
                    target_os = "dragonfly",
                    target_os = "freebsd",
                    target_os = "netbsd",
                    target_os = "openbsd"
                ))]
                let mut builder = WebViewBuilder::new_gtk(window.gtk_window())
                    .with_navigation_handler(move |url| {
                        // eprintln!("Webview {i} is navigating to {url}");
                        true
                    })
                    .with_on_page_load_handler(move |inner, url| {
                        let hook = match inner {
                            PageLoadEvent::Started => PBHook {
                                pool_item: i,
                                event: PBWebviewEvent::PageLoadStart { url },
                            },
                            PageLoadEvent::Finished => PBHook {
                                pool_item: i,
                                event: PBWebviewEvent::PageLoadFinish { url },
                            },
                        };

                        if this_proxy
                            .send_event(Box::new(PBEvent::Hook(hook)))
                            .is_err()
                        {
                            panic!("todo");
                        };
                    });

                if let Some(js) = &params.init_script {
                    builder = builder.with_initialization_script(&js);
                }

                let webview = builder.build().expect("Webview should create successfully");

                platforms::Platform::enhance_webview(&webview);

                PoolItem {
                    id: i,
                    window,
                    webview,
                    assigned_to: None,
                    pending_responses: HashMap::new(),
                }
            })
            .collect();

        Self {
            items: pool_items,
            assignments: HashMap::new(),
            next_assignment: 0,
            waiting_for_windows: VecDeque::new(),
        }
    }

    fn get_assigned_window(&mut self, window_id: u32) -> Result<&mut PoolItem, ()> {
        let Some(window_assignment) = self.assignments.get(&window_id) else {
            return Err(());
        };

        let window_in_pool = self.items.get_mut(window_assignment.pool_index).unwrap();

        Ok(window_in_pool)
    }

    fn release_assigned_window(&mut self, window_id: u32) {
        self.assignments.remove(&window_id);
    }

    fn assign_window(&mut self, pool_index: usize) -> u32 {
        let window_id = self.next_assignment;
        self.next_assignment += 1;

        self.assignments
            .insert(window_id, WindowReference { pool_index });

        self.items.get_mut(pool_index).unwrap().assigned_to = Some(window_id);

        window_id
    }
}

fn parse_buf_or_write_error(
    buf: Vec<u8>,
    outgoing_tx: &Sender<PBResponse>,
) -> Result<PBRequest, ()> {
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
        return Err(());
    };

    match serde_json::from_slice::<PBRequest>(&decoded) {
        Ok(msg) => Ok(msg),
        Err(e) => {
            let error = match std::str::from_utf8(&decoded[..]) {
                Ok(msg) => PBResponsePayload::Error {
                    original_message: Some(msg.to_string()),
                    message: format!("{e}"),
                },
                Err(_) => PBResponsePayload::Error {
                    original_message: None,
                    message:
                        "Pagebrowse was unable to parse the message it was provided via the service"
                            .to_string(),
                },
            };

            outgoing_tx
                .send(PBResponse {
                    message_id: None,
                    payload: error,
                })
                .expect("Channel is open");

            Err(())
        }
    }
}

fn start_listening(proxy: EventLoopProxy<Box<PBEvent>>, outgoing_tx: Sender<PBResponse>) {
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

            if let Ok(msg) = parse_buf_or_write_error(buf, &outgoing_tx) {
                _ = event_loop_proxy.send_event(Box::new(PBEvent::Request(msg)));
            }
        }
    });
}

fn listen_for_init(outgoing_tx: Sender<PBResponse>) -> InitializationParams {
    let mut stdin = std::io::stdin().lock();

    loop {
        let mut buf = vec![];
        stdin.read_until(b',', &mut buf).unwrap();
        if buf.pop().is_none() {
            // EOF Reached
            std::process::exit(0);
        }

        if let Ok(msg) = parse_buf_or_write_error(buf, &outgoing_tx) {
            match msg.payload {
                PBRequestPayload::Initialize(params) => {
                    outgoing_tx
                        .send(PBResponse {
                            message_id: msg.message_id,
                            payload: PBResponsePayload::OperationComplete,
                        })
                        .expect("handle this error one day");

                    return params;
                }
                _ => {
                    outgoing_tx
                    .send(PBResponse {
                        message_id: msg.message_id,
                        payload: PBResponsePayload::Error {
                            original_message: None,
                            message: "Initialize message has not been sent, Pagebrowse is not yet ready".into(),
                        },
                    })
                    .expect("Channel is open");
                }
            }
        }
    }
}

fn main() {
    let (outgoing_tx, outgoing_rx) = std::sync::mpsc::channel::<PBResponse>();

    let _options = get_cli_matches();
    // No options currently used
    // TODO: Add CLI option for which communication method to use (network / stdio / etc)

    std::thread::spawn(move || {
        let mut stdout = std::io::stdout().lock();

        loop {
            let msg = match outgoing_rx.recv() {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("errored: {e}");
                    continue;
                }
            };

            let encoded = general_purpose::STANDARD.encode(serde_json::to_vec(&msg).unwrap());

            stdout.write_all(encoded.as_bytes()).unwrap();
            stdout.write(b",").unwrap();
            stdout.flush().unwrap();
        }
    });

    let event_loop = platforms::Platform::setup();
    let proxy = event_loop.create_proxy();

    let intial_params = listen_for_init(outgoing_tx.clone());
    let mut pool = Pool::new(intial_params, &event_loop, proxy.clone());

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::UserEvent(evt) => {
                handle_event(*evt, &mut pool, outgoing_tx.clone(), proxy.clone());
            }
            Event::NewEvents(StartCause::Init) => {
                start_listening(proxy.clone(), outgoing_tx.clone());
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            _ => (),
        };

        #[cfg(target_os = "linux")]
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
    });
}

fn handle_event(
    evt: PBEvent,
    pool: &mut Pool,
    outgoing_tx: Sender<PBResponse>,
    proxy: EventLoopProxy<Box<PBEvent>>,
) {
    match evt {
        PBEvent::Request(msg) => handle_message(msg, pool, outgoing_tx, proxy),
        PBEvent::Hook(hook) => handle_hook(hook, pool, outgoing_tx, proxy),
    }
}

fn handle_hook(
    hook: PBHook,
    pool: &mut Pool,
    outgoing_tx: Sender<PBResponse>,
    proxy: EventLoopProxy<Box<PBEvent>>,
) {
    let window_in_pool = pool
        .items
        .get_mut(hook.pool_item)
        .expect("Pool is behaving");

    if let Some(resp) = window_in_pool.pending_responses.remove(&hook.event) {
        outgoing_tx.send(resp).expect("handle this error one day");
    }
}

fn handle_message(
    msg: PBRequest,
    pool: &mut Pool,
    outgoing_tx: Sender<PBResponse>,
    proxy: EventLoopProxy<Box<PBEvent>>,
) {
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
        PBRequestPayload::Initialize(_) => {
            outgoing_tx
                .send(PBResponse {
                    message_id: msg.message_id,
                    payload: PBResponsePayload::Error {
                        original_message: None,
                        message: "Pagebrowse is already initialized".into(),
                    },
                })
                .expect("Channel is open");
        }
        PBRequestPayload::NewWindow => {
            let Some((pool_index, item)) = pool
                .items
                .iter_mut()
                .enumerate()
                .find(|(_, item)| item.assigned_to.is_none())
            else {
                pool.waiting_for_windows.push_back(message_id);
                return;
            };

            let assigned_to = pool.assign_window(pool_index);

            outgoing_tx
                .send(PBResponse {
                    message_id: Some(message_id),
                    payload: PBResponsePayload::NewWindowCreated { id: assigned_to },
                })
                .expect("handle this error one day");
        }
        PBRequestPayload::ReleaseWindow { window_id } => {
            let window_in_pool = pool
                .get_assigned_window(window_id)
                .expect("Consumer is behaving");

            window_in_pool.assigned_to = None;
            window_in_pool.pending_responses.clear();
            let released_id = window_in_pool.id;

            pool.release_assigned_window(window_id);

            outgoing_tx
                .send(PBResponse {
                    message_id: Some(message_id),
                    payload: PBResponsePayload::OperationComplete,
                })
                .expect("handle this error one day");

            if let Some(waiting) = pool.waiting_for_windows.pop_front() {
                let assigned_to = pool.assign_window(released_id);

                outgoing_tx
                    .send(PBResponse {
                        message_id: Some(waiting),
                        payload: PBResponsePayload::NewWindowCreated { id: assigned_to },
                    })
                    .expect("handle this error one day");
            }
        }
        PBRequestPayload::Navigate {
            window_id,
            url,
            wait_for_load,
        } => {
            let window_in_pool = pool
                .get_assigned_window(window_id)
                .expect("Consumer is behaving");

            let response = PBResponse {
                message_id: Some(message_id),
                payload: PBResponsePayload::OperationComplete,
            };

            if wait_for_load {
                window_in_pool.pending_responses.insert(
                    PBWebviewEvent::PageLoadFinish { url: url.clone() },
                    response.clone(),
                );
            }

            window_in_pool.webview.load_url(&url);

            if !wait_for_load {
                outgoing_tx
                    .send(response)
                    .expect("handle this error one day");
            }
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

            let x = (window_in_pool.assigned_to.unwrap() % 4) * 1920 / 2;
            let y = ((window_in_pool.assigned_to.unwrap() / 4) % 4) * 1080 / 2;

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

            let res_callback = move |output: String| {
                outgoing_tx
                    .send(PBResponse {
                        message_id: Some(message_id),
                        payload: PBResponsePayload::ScriptEvaluated { output },
                    })
                    .expect("handle this error one day");
            };

            let js = format!("{script}\n");
            platforms::Platform::run_js(&window_in_pool.webview, &js, res_callback);
        }
        PBRequestPayload::Screenshot { window_id, path } => {
            let window_in_pool = pool
                .get_assigned_window(window_id)
                .expect("Consumer is behaving");

            let screenshot_callback = move |bytes: &[u8]| {
                eprintln!("Called back the bytes");
                std::fs::write(path.clone(), bytes).unwrap();
                // let image = image::load_from_memory(bytes).expect("Cursor io never fails");

                // image.save(path.clone()).unwrap();

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
