use serde::{Deserialize, Serialize};

pub mod options;
pub mod platforms;

#[derive(Debug)]
pub enum PBEvent {
    Request(PBRequest),
    Hook(PBHook),
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum PBWebviewEvent {
    PageLoadStart { url: String },
    PageLoadFinish { url: String },
}

#[derive(Debug, PartialEq)]
pub struct PBHook {
    pub pool_item: usize,
    pub event: PBWebviewEvent,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct InitializationParams {
    pub pool_size: usize,
    pub visible: bool,
    pub init_script: Option<String>,
}

mod requests {
    use super::*;

    #[derive(Debug, Deserialize, Serialize)]
    pub struct PBRequest {
        pub message_id: Option<u32>,
        pub payload: PBRequestPayload,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub enum PBRequestPayload {
        Tester(String),
        Initialize(InitializationParams),
        NewWindow,
        ReleaseWindow {
            window_id: u32,
        },
        Navigate {
            window_id: u32,
            url: String,
            wait_for_load: bool,
        },
        ResizeWindow {
            window_id: u32,
            width: usize,
            height: usize,
        },
        EvaluateScript {
            window_id: u32,
            script: String,
        },
        Screenshot {
            window_id: u32,
            path: String,
        },
    }
}

mod responses {
    use super::*;

    #[derive(Clone, Debug, Deserialize, Serialize)]
    pub struct PBResponse {
        pub message_id: Option<u32>,
        pub payload: PBResponsePayload,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub enum PBResponsePayload {
        Error {
            original_message: Option<String>,
            message: String,
        },
        Tester(String),
        NewWindowCreated {
            id: u32,
        },
        ScriptEvaluated {
            output: String,
        },
        OperationComplete,
    }
}

pub use requests::*;
pub use responses::*;
use wry::PageLoadEvent;
