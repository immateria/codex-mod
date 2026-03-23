use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use code_core::config::Config;
use code_core::config::ConfigOverrides;
use code_core::config_types::ReasoningEffort;
use code_core::debug_logger::DebugLogger;
use code_core::protocol::SandboxPolicy;
use code_core::{AuthManager, ModelClient, Prompt, ResponseEvent, TextFormat};
use code_login::AuthMode;
use code_protocol::models::{ContentItem, ResponseItem};
use futures::StreamExt;
use serde::Deserialize;
use serde_json::{self, json, Value};
use tracing::debug;
use uuid::Uuid;

use crate::app_event::{
    AppEvent,
    Redacted,
    TerminalAfter,
    TerminalCommandGate,
    TerminalRunController,
    TerminalRunEvent,
};
use crate::app_event_sender::AppEventSender;

include!("preamble.rs");
include!("session_start.rs");
include!("guided_loop.rs");
include!("model.rs");
include!("command_helpers.rs");
