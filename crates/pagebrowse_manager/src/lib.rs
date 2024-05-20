use serde::{Deserialize, Serialize};

use pagebrowse_types::{
    InitializationParams, PBRequest, PBRequestPayload, PBResponse, PBResponsePayload,
};

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

use wry::PageLoadEvent;
