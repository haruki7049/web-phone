//! Application state and background handlers for TUI client.

use std::collections::VecDeque;
use tokio::sync::{mpsc, oneshot};
use wpapi::{Configuration, UserAddress, session::ClientSession};

/// Background events sent to the TUI event loop.
#[derive(Debug)]
pub enum AppEvent {
    /// Temporary UserAddress assigned by server.
    ServerAddressAssigned(UserAddress),
    /// Active session ended with result message or error.
    SessionEnded {
        /// Session identifier.
        session_id: u64,
        /// Result of session execution.
        result: Result<String, String>,
    },
    /// Call request accepted by peer.
    CallAccepted {
        /// Session identifier.
        session_id: u64,
        /// Target peer address string.
        target: String,
    },
    /// Call request rejected by peer.
    CallRejected {
        /// Session identifier.
        session_id: u64,
        /// Target peer address string.
        target: String,
        /// Reason for rejection.
        reason: String,
    },
    /// Active call ended by remote peer.
    CallEnded {
        /// Target peer address string.
        target: String,
    },
    /// Incoming call prompt request.
    IncomingCall {
        /// Short ID of caller.
        from: String,
        /// Oneshot channel to accept (true) or reject (false).
        responder: oneshot::Sender<bool>,
    },
    /// List of registered user addresses retrieved from daemon.
    RegisteredAddresses(Vec<UserAddress>),
    /// Microphone input and speaker output energy levels.
    AudioLevels {
        /// Input audio RMS level (0.0..=1.0).
        input: f32,
        /// Output audio RMS level (0.0..=1.0).
        output: f32,
    },
    /// Log message line to record.
    Log(String),
}

/// UI Interaction Modes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    /// Normal navigation mode.
    Normal,
    /// Entering target user address for 1-to-1 call.
    CallInput,
    /// Entering room ID for SFU group call.
    RoomInput,
    /// Prompting for incoming call accept/reject.
    IncomingCall,
}

/// Connection and call state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallState {
    /// Client idle and disconnected.
    Idle,
    /// Standby mode listening for incoming calls.
    Standby,
    /// Connecting to 1-to-1 call target.
    Connecting(String),
    /// Active 1-to-1 call session with target user.
    InCall(String),
    /// Active SFU group room session.
    InRoom(String),
}

/// Application state for the TUI client.
pub struct TuiApp {
    /// Client configuration settings.
    pub config: Configuration,
    /// Current UI input mode.
    pub input_mode: InputMode,
    /// Current call/connection state.
    pub call_state: CallState,
    /// Counter for active session IDs.
    pub current_session_id: u64,
    /// Local client user address.
    pub my_address: Option<UserAddress>,
    /// Text input buffer for modal prompts.
    pub input_buffer: String,
    /// Activity log message queue.
    pub logs: VecDeque<String>,
    /// List of registered peer addresses.
    pub registered_addresses: Vec<UserAddress>,
    /// Microphone mute toggle status.
    pub is_muted: bool,
    /// Measured input audio level.
    pub input_level: f32,
    /// Measured output audio level.
    pub output_level: f32,
    /// Incoming call caller ID if prompting.
    pub incoming_from: Option<String>,
    /// Incoming call responder channel sender.
    pub incoming_responder: Option<oneshot::Sender<bool>>,
    /// Cancellation channel sender for standby task.
    pub cancel_tx: Option<oneshot::Sender<()>>,
    /// Active client session handle.
    pub session: Option<ClientSession>,
    /// Application event channel sender.
    pub event_tx: mpsc::Sender<AppEvent>,
}

impl TuiApp {
    /// Create a new `TuiApp` state with default identity keypair.
    pub fn new(config: Configuration, event_tx: mpsc::Sender<AppEvent>) -> Self {
        Self::with_keypair(config, event_tx, None)
    }

    /// Create a new `TuiApp` state with optional custom identity keypair.
    pub fn with_keypair(
        config: Configuration,
        event_tx: mpsc::Sender<AppEvent>,
        keypair: Option<wpapi::UserKeypair>,
    ) -> Self {
        let session = if let Some(kp) = keypair {
            ClientSession::with_keypair(kp)
        } else {
            ClientSession::new()
        };
        session.set_allow_echoback(config.audio.allow_echoback);
        let mut app = Self {
            config,
            input_mode: InputMode::Normal,
            call_state: CallState::Idle,
            current_session_id: 0,
            my_address: None,
            input_buffer: String::new(),
            logs: VecDeque::with_capacity(100),
            registered_addresses: Vec::new(),
            is_muted: false,
            input_level: 0.0,
            output_level: 0.0,
            incoming_from: None,
            incoming_responder: None,
            cancel_tx: None,
            session: Some(session),
            event_tx,
        };
        app.add_log(
            "TUI Application initialized. Press [s] for standby, [c] to call, [r] for room."
                .to_string(),
        );
        app
    }

    /// Add a log message string to the activity log queue.
    pub fn add_log(&mut self, msg: String) {
        if self.logs.len() >= 100 {
            self.logs.pop_front();
        }
        self.logs.push_back(msg);
    }

    /// Increment and return the next session ID.
    pub fn next_session_id(&mut self) -> u64 {
        self.current_session_id += 1;
        self.current_session_id
    }

    /// Hangup current call/session if active.
    pub fn hangup(&mut self) {
        if let Some(ref session) = self.session {
            if let Some(target) = session.get_target_address() {
                let hangup_pkt = wpapi::protocol::ProtocolPacket::CallHangup {
                    target_address: target,
                };
                let session_clone = session.clone();
                tokio::spawn(async move {
                    let _ = session_clone.send_packet(&hangup_pkt).await;
                });
                session.set_target_address(None);
            }
            if let Some(room) = session.get_room_address() {
                let leave_pkt =
                    wpapi::protocol::ProtocolPacket::RoomLeaveRequest { room_address: room };
                let session_clone = session.clone();
                tokio::spawn(async move {
                    let _ = session_clone.send_packet(&leave_pkt).await;
                });
                session.set_room_address(None);
            }
        }
        self.input_level = 0.0;
        self.output_level = 0.0;
        if self.cancel_tx.is_some() {
            self.call_state = CallState::Standby;
            self.add_log("Returned to Standby Mode.".to_string());
        } else {
            self.call_state = CallState::Idle;
            self.add_log("Returned to IDLE state.".to_string());
        }
    }
}

pub(crate) fn setup_incoming_call_handler(
    session: &ClientSession,
    event_tx: mpsc::Sender<AppEvent>,
) {
    let (inc_tx, mut inc_rx) = mpsc::channel::<(UserAddress, oneshot::Sender<bool>)>(10);
    session.set_incoming_call_handler(inc_tx);

    tokio::spawn(async move {
        while let Some((caller_addr, responder)) = inc_rx.recv().await {
            let _ = event_tx
                .send(AppEvent::IncomingCall {
                    from: caller_addr.short_id().to_string(),
                    responder,
                })
                .await;
        }
    });
}

pub(crate) fn setup_call_notification_handler(
    session: &ClientSession,
    event_tx: mpsc::Sender<AppEvent>,
    session_id: u64,
) {
    let (notif_tx, mut notif_rx) = mpsc::channel::<wpapi::session::CallNotification>(10);
    session.set_call_notification_handler(notif_tx);

    tokio::spawn(async move {
        while let Some(notif) = notif_rx.recv().await {
            match notif {
                wpapi::session::CallNotification::Accepted(target_addr) => {
                    let _ = event_tx
                        .send(AppEvent::CallAccepted {
                            session_id,
                            target: target_addr.short_id().to_string(),
                        })
                        .await;
                }
                wpapi::session::CallNotification::Rejected(target_addr) => {
                    let _ = event_tx
                        .send(AppEvent::CallRejected {
                            session_id,
                            target: target_addr.short_id().to_string(),
                            reason: "Connection rejected by target user".to_string(),
                        })
                        .await;
                }
                wpapi::session::CallNotification::Error(target_addr, reason) => {
                    let _ = event_tx
                        .send(AppEvent::CallRejected {
                            session_id,
                            target: target_addr.short_id().to_string(),
                            reason,
                        })
                        .await;
                }
                wpapi::session::CallNotification::Hangup(target_addr) => {
                    let _ = event_tx
                        .send(AppEvent::CallEnded {
                            target: target_addr.short_id().to_string(),
                        })
                        .await;
                }
            }
        }
    });
}

pub(crate) fn stop_standby(app: &mut TuiApp) {
    if let Some(tx) = app.cancel_tx.take() {
        let _ = tx.send(());
    }
    if let Some(ref session) = app.session {
        session.set_target_address(None);
        session.set_room_address(None);
    }
    app.call_state = CallState::Idle;
    app.add_log("Standby mode stopped. Entered IDLE state.".to_string());
}

pub(crate) fn start_standby(app: &mut TuiApp) {
    if app.cancel_tx.is_some() {
        stop_standby(app);
        return;
    }

    let session_id = app.next_session_id();
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    let (assigned_tx, mut assigned_rx) = mpsc::channel::<UserAddress>(1);
    let event_tx = app.event_tx.clone();
    let config = app.config.clone();

    app.cancel_tx = Some(cancel_tx);
    app.call_state = CallState::Standby;
    app.add_log("Starting standby mode (listening for incoming calls)...".to_string());

    let session = app.session.clone().unwrap_or_default();
    setup_incoming_call_handler(&session, event_tx.clone());
    setup_call_notification_handler(&session, event_tx.clone(), session_id);

    let session_clone = session.clone();
    tokio::spawn(async move {
        let mut assigned = false;
        for _ in 0..100 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            if let Some(addr) = session_clone.get_user_address() {
                tracing::info!("Daemon user address assigned successfully: {}", addr);
                let _ = assigned_tx.send(addr).await;
                assigned = true;
                break;
            }
        }
        if !assigned {
            tracing::warn!(
                "Daemon address assignment (ClientAssignment) timed out (5s). WebRTC DataChannel/UDP connection to daemon was not established."
            );
        }
    });

    let event_tx_addr = event_tx.clone();
    tokio::spawn(async move {
        if let Some(addr) = assigned_rx.recv().await {
            let _ = event_tx_addr
                .send(AppEvent::ServerAddressAssigned(addr))
                .await;
        }
    });

    let event_tx_session = event_tx;
    tokio::spawn(async move {
        let res = wpapi::call::start_call_with_session(&config, None, Some(cancel_rx), &session)
            .await
            .map(|_| "Standby ended".to_string())
            .map_err(|e| e.to_string());
        let _ = event_tx_session
            .send(AppEvent::SessionEnded {
                session_id,
                result: res,
            })
            .await;
    });
}

pub(crate) fn start_call(app: &mut TuiApp, target_id: String) {
    let clean_id = target_id.trim().to_string();
    if clean_id.len() < 12 {
        app.add_log(format!(
            "Target Short ID must be at least 12 characters (got {}).",
            clean_id.len()
        ));
        app.call_state = CallState::Standby;
        return;
    }
    let target_addr = UserAddress::new(clean_id.clone());
    app.call_state = CallState::Connecting(clean_id.clone());
    app.add_log(format!("Initiating call to target User ID: {}", clean_id));

    if let Some(ref session) = app.session {
        session.set_target_address(Some(target_addr.clone()));
        let init_packet = wpapi::protocol::ProtocolPacket::ClientTargetedAudio {
            target_address: target_addr,
            codec_id: wpapi::protocol::CODEC_OPUS,
            audio_data: vec![],
        };
        let session_clone = session.clone();
        tokio::spawn(async move {
            if let Err(e) = session_clone.send_packet(&init_packet).await {
                tracing::error!("Failed to send call initiation packet: {}", e);
            }
        });
    }
}

pub(crate) fn start_room(app: &mut TuiApp, room_id: String) {
    let room_addr = UserAddress::from_room_id(&room_id);
    app.call_state = CallState::InRoom(room_id.clone());
    app.add_log(format!(
        "Joining SFU group room: {} ({})",
        room_id,
        room_addr.short_id()
    ));

    if let Some(ref session) = app.session {
        session.set_room_address(Some(room_addr.clone()));
        let join_packet = wpapi::protocol::ProtocolPacket::RoomJoinRequest {
            room_address: room_addr,
        };
        let session_clone = session.clone();
        tokio::spawn(async move {
            if let Err(e) = session_clone.send_packet(&join_packet).await {
                tracing::error!("Failed to send room join packet: {}", e);
            }
        });
    }
}

pub(crate) fn fetch_registered_addresses(app: &mut TuiApp) {
    let event_tx = app.event_tx.clone();
    let config = app.config.clone();
    app.add_log("Fetching registered user addresses from daemon...".to_string());

    tokio::spawn(async move {
        let endpoint = format!("{}/addresses", config.server_url());
        let client = reqwest::Client::new();
        let keypair = wpapi::UserKeypair::generate();
        let (_, auth_hdr) = wpapi::build_authorization_header(&keypair, "");
        match client
            .get(&endpoint)
            .header(reqwest::header::AUTHORIZATION, auth_hdr)
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    match resp.json::<Vec<UserAddress>>().await {
                        Ok(addrs) => {
                            let _ = event_tx.send(AppEvent::RegisteredAddresses(addrs)).await;
                        }
                        Err(e) => {
                            let _ = event_tx
                                .send(AppEvent::Log(format!("Failed to parse response: {}", e)))
                                .await;
                        }
                    }
                } else {
                    let _ = event_tx
                        .send(AppEvent::Log(format!(
                            "Server returned HTTP status {}",
                            resp.status()
                        )))
                        .await;
                }
            }
            Err(e) => {
                let _ = event_tx
                    .send(AppEvent::Log(format!("Failed to connect to daemon: {}", e)))
                    .await;
            }
        }
    });
}

pub(crate) fn handle_app_event(app: &mut TuiApp, evt: AppEvent) {
    match evt {
        AppEvent::ServerAddressAssigned(addr) => {
            app.add_log(format!("Server assigned Short ID: {}", addr.short_id()));
            app.my_address = Some(addr);
        }
        AppEvent::SessionEnded { session_id, result } => {
            match result {
                Ok(target) => app.add_log(format!("Session ended: {}", target)),
                Err(err) => app.add_log(format!("Session error: {}", err)),
            }
            if app.current_session_id == session_id {
                app.call_state = CallState::Idle;
                app.cancel_tx = None;
            }
        }
        AppEvent::CallAccepted {
            session_id: _,
            target,
        } => {
            app.add_log(format!("Call connected with target {}!", target));
            app.call_state = CallState::InCall(target);
        }
        AppEvent::CallRejected {
            session_id: _,
            target,
            reason,
        } => {
            app.add_log(format!("Call to {} failed: {}", target, reason));
            app.call_state = if app.cancel_tx.is_some() {
                CallState::Standby
            } else {
                CallState::Idle
            };
            if let Some(ref session) = app.session {
                session.set_target_address(None);
            }
        }
        AppEvent::CallEnded { target } => {
            app.add_log(format!("Call with target {} ended.", target));
            app.call_state = if app.cancel_tx.is_some() {
                CallState::Standby
            } else {
                CallState::Idle
            };
            if let Some(ref session) = app.session {
                session.set_target_address(None);
                session.set_room_address(None);
            }
        }
        AppEvent::IncomingCall { from, responder } => {
            app.add_log(format!("Incoming call request from: {}", from));
            let is_outgoing_target = match &app.call_state {
                CallState::Connecting(target) => {
                    target == &from || from.starts_with(target) || target.starts_with(&from)
                }
                CallState::InCall(target) => {
                    target == &from || from.starts_with(target) || target.starts_with(&from)
                }
                _ => false,
            };

            if app.config.client.auto_accept || is_outgoing_target {
                let _ = responder.send(true);
                app.add_log("Auto-accepted incoming call.".to_string());
                app.call_state = CallState::InCall(from);
            } else {
                app.incoming_from = Some(from);
                app.incoming_responder = Some(responder);
                app.input_mode = InputMode::IncomingCall;
            }
        }
        AppEvent::RegisteredAddresses(addrs) => {
            app.add_log(format!("Retrieved {} registered address(es).", addrs.len()));
            app.registered_addresses = addrs;
        }
        AppEvent::AudioLevels { input, output } => {
            app.input_level = input;
            app.output_level = output;
        }
        AppEvent::Log(msg) => {
            app.add_log(msg);
        }
    }
}

pub(crate) fn renew_identity(app: &mut TuiApp) {
    if let Some(tx) = app.cancel_tx.take() {
        let _ = tx.send(());
    }
    app.call_state = CallState::Idle;
    app.session = Some(ClientSession::with_keypair(wpapi::UserKeypair::generate()));
    app.my_address = None;
    app.add_log("Generated new Ed25519 identity keypair. Reconnecting to daemon...".to_string());
    start_standby(app);
}
