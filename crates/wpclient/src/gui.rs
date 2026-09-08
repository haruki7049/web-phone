//! Iced GUI application module for `wpclient`.
//!
//! Provides a modern interactive graphical user interface featuring real-time audio volume meters,
//! interactive mute/unmute toggles, active room participant indicators, registered address discovery,
//! user short ID generation/display, call approval/rejection UI, and structured audio device listing complying with Issue #16.

use iced::widget::{
    button, column, container, progress_bar, row, scrollable, text, text_input, vertical_space,
};
use iced::{Alignment, Element, Length, Task, Theme};
use std::sync::OnceLock;
use wpapi::{Configuration, UserAddress};

pub static TOKIO_HANDLE: OnceLock<tokio::runtime::Handle> = OnceLock::new();

/// Spawn a task on the global Tokio runtime handle to execute async I/O within Tokio reactor context.
pub async fn spawn_tokio<F, T>(future: F) -> Result<T, String>
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    if let Some(handle) = TOKIO_HANDLE.get() {
        handle
            .spawn(future)
            .await
            .map_err(|e| format!("Tokio task join error: {}", e))
    } else {
        Ok(future.await)
    }
}

/// Main application state for wpclient iced GUI.
#[derive(Debug)]
pub struct WpClientGui {
    config: Configuration,
    my_address: UserAddress,
    target_address_input: String,
    room_address_input: String,
    call_state: CallStatus,
    auto_accept: bool,
    incoming_call_from: Option<UserAddress>,
    is_muted: bool,
    input_volume: f32,  // 0.0 to 1.0 (VU Meter)
    output_volume: f32, // 0.0 to 1.0 (VU Meter)
    participants: Vec<String>,
    active_speakers: Vec<String>,
    registered_addresses: Option<Vec<UserAddress>>,
    input_devices: Vec<String>,
    output_devices: Vec<String>,
    status_info: String,
    call_cancel_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallStatus {
    Idle,
    Connecting,
    InCall { target: String },
    InRoom { room: String },
}

/// Messages emitted by GUI widgets and async tasks.
#[derive(Debug, Clone)]
pub enum Message {
    GenerateNewIdPressed,
    ToggleAutoAcceptPressed,
    AcceptIncomingCallPressed,
    RejectIncomingCallPressed,
    TargetAddressChanged(String),
    RoomAddressChanged(String),
    StartCallPressed,
    StartRoomPressed,
    HangupPressed,
    ToggleMutePressed,
    FetchAddressesPressed,
    AddressesFetched(Result<Vec<UserAddress>, String>),
    FetchDevicesPressed,
    DevicesFetched(Result<(Vec<String>, Vec<String>), String>),
    CallUserSelected(String),
    VolumeUpdated { input: f32, output: f32 },
    ParticipantListUpdated(Vec<String>),
    ActiveSpeakersUpdated(Vec<String>),
    CallResult(Result<String, String>),
    RoomResult(Result<String, String>),
    ServerAssignedAddressReceived(Result<UserAddress, String>),
}

impl WpClientGui {
    pub fn new(config: Configuration) -> (Self, Task<Message>) {
        let my_address = UserAddress::generate_from_time();
        let app = Self {
            config,
            my_address,
            target_address_input: String::new(),
            room_address_input: String::new(),
            call_state: CallStatus::Idle,
            auto_accept: true, // Default to true for smooth WebRTC connection
            incoming_call_from: None,
            is_muted: false,
            input_volume: 0.25,
            output_volume: 0.10,
            participants: vec![],
            active_speakers: vec![],
            registered_addresses: None,
            input_devices: vec![],
            output_devices: vec![],
            status_info: "Connecting to server and registering...".to_string(),
            call_cancel_tx: None,
        };

        // Automatically enumerate audio hardware devices at startup
        let fetch_devices_task = Task::perform(
            async move {
                spawn_tokio(async move {
                    wpapi::audio::get_device_names()
                        .map_err(|e| format!("Failed to fetch devices: {}", e))
                })
                .await
                .unwrap_or_else(Err)
            },
            Message::DevicesFetched,
        );

        // Auto-connect standby mode on startup to immediately obtain server-assigned UserAddress
        let auto_connect_task = Task::done(Message::StartCallPressed);

        (
            app,
            Task::batch(vec![fetch_devices_task, auto_connect_task]),
        )
    }

    pub fn title(&self) -> String {
        format!("web-phone Desktop (wpclient) - {:?}", self.call_state)
    }

    pub fn theme(&self) -> Theme {
        Theme::Dark
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::GenerateNewIdPressed => {
                self.my_address = UserAddress::generate_from_time();
                self.status_info = format!(
                    "Re-registering with server for Short ID: {}",
                    self.my_address.short_id()
                );
                Task::done(Message::StartCallPressed)
            }
            Message::ServerAssignedAddressReceived(res) => {
                if let Ok(assigned_addr) = res {
                    self.my_address = assigned_addr.clone();
                    self.status_info = format!(
                        "Registered with wpdaemon server! Short ID: {}",
                        assigned_addr.short_id()
                    );
                }
                Task::none()
            }
            Message::ToggleAutoAcceptPressed => {
                self.auto_accept = !self.auto_accept;
                self.config.auto_accept = self.auto_accept;
                self.status_info = if self.auto_accept {
                    "Auto-accept incoming calls: ENABLED".to_string()
                } else {
                    "Auto-accept incoming calls: DISABLED (Prompt GUI on incoming call)".to_string()
                };
                Task::none()
            }
            Message::AcceptIncomingCallPressed => {
                if let Some(caller) = self.incoming_call_from.take() {
                    self.call_state = CallStatus::InCall {
                        target: caller.to_string(),
                    };
                    self.participants = vec![caller.to_string()];
                    self.status_info = format!("Accepted incoming call from {}", caller.short_id());
                }
                Task::none()
            }
            Message::RejectIncomingCallPressed => {
                if let Some(caller) = self.incoming_call_from.take() {
                    self.status_info = format!("Rejected incoming call from {}", caller.short_id());
                }
                Task::none()
            }
            Message::TargetAddressChanged(val) => {
                self.target_address_input = val;
                Task::none()
            }
            Message::RoomAddressChanged(val) => {
                self.room_address_input = val;
                Task::none()
            }
            Message::StartCallPressed => {
                let target_str = self.target_address_input.trim().to_string();
                let target_opt = if target_str.is_empty() {
                    None
                } else {
                    Some(UserAddress::new(target_str.clone()))
                };

                // Cancel any ongoing call before starting new one
                if let Some(tx) = self.call_cancel_tx.take() {
                    let _ = tx.send(());
                }

                let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
                self.call_cancel_tx = Some(cancel_tx);

                let display_target = target_opt
                    .as_ref()
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "Standby Mode (Listening)".to_string());

                self.call_state = CallStatus::InCall {
                    target: display_target.clone(),
                };
                self.participants = vec![display_target.clone()];
                self.input_volume = 0.45;
                self.output_volume = 0.30;

                self.status_info = match &target_opt {
                    Some(t) => format!("1-to-1 WebRTC call active with {}...", t.short_id()),
                    None => "Standby mode active (listening for incoming calls)...".to_string(),
                };

                let mut config = self.config.clone();
                config.auto_accept = self.auto_accept;

                let (assigned_tx, mut assigned_rx) = tokio::sync::mpsc::channel::<UserAddress>(1);

                Task::batch(vec![
                    Task::perform(
                        async move {
                            spawn_tokio(async move {
                                if let Some(addr) = assigned_rx.recv().await {
                                    Ok(addr)
                                } else {
                                    Err("No address assigned".to_string())
                                }
                            })
                            .await
                            .unwrap_or_else(Err)
                        },
                        Message::ServerAssignedAddressReceived,
                    ),
                    Task::perform(
                        async move {
                            let res = spawn_tokio(async move {
                                let session = wpapi::session::ClientSession::new();
                                let session_clone = session.clone();

                                tokio::spawn(async move {
                                    for _ in 0..100 {
                                        tokio::time::sleep(std::time::Duration::from_millis(50))
                                            .await;
                                        if let Some(addr) = session_clone.get_user_address() {
                                            let _ = assigned_tx.send(addr).await;
                                            break;
                                        }
                                    }
                                });

                                wpapi::call::start_call_with_session(
                                    &config,
                                    target_opt,
                                    Some(cancel_rx),
                                    &session,
                                )
                                .await
                                .map_err(|e| format!("Call error: {}", e))
                            })
                            .await
                            .unwrap_or_else(Err);

                            match res {
                                Ok(()) => Ok(display_target),
                                Err(e) => Err(e),
                            }
                        },
                        Message::CallResult,
                    ),
                ])
            }
            Message::StartRoomPressed => {
                let room_str = self.room_address_input.trim().to_string();
                if room_str.is_empty() {
                    self.status_info = "Room ID cannot be empty.".to_string();
                    return Task::none();
                }
                let room_addr = UserAddress::new(room_str.clone());

                if let Some(tx) = self.call_cancel_tx.take() {
                    let _ = tx.send(());
                }

                let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
                self.call_cancel_tx = Some(cancel_tx);
                let display_room = room_addr.to_string();

                self.call_state = CallStatus::InRoom {
                    room: display_room.clone(),
                };
                self.input_volume = 0.50;
                self.output_volume = 0.60;
                self.status_info = format!("Joined SFU room {}...", room_addr.short_id());

                let mut config = self.config.clone();
                config.auto_accept = self.auto_accept;

                let (assigned_tx, mut assigned_rx) = tokio::sync::mpsc::channel::<UserAddress>(1);

                Task::batch(vec![
                    Task::perform(
                        async move {
                            spawn_tokio(async move {
                                if let Some(addr) = assigned_rx.recv().await {
                                    Ok(addr)
                                } else {
                                    Err("No address assigned".to_string())
                                }
                            })
                            .await
                            .unwrap_or_else(Err)
                        },
                        Message::ServerAssignedAddressReceived,
                    ),
                    Task::perform(
                        async move {
                            let res = spawn_tokio(async move {
                                let session = wpapi::session::ClientSession::new();
                                let session_clone = session.clone();

                                tokio::spawn(async move {
                                    for _ in 0..100 {
                                        tokio::time::sleep(std::time::Duration::from_millis(50))
                                            .await;
                                        if let Some(addr) = session_clone.get_user_address() {
                                            let _ = assigned_tx.send(addr).await;
                                            break;
                                        }
                                    }
                                });

                                wpapi::call::start_room_call_with_session(
                                    &config,
                                    room_addr,
                                    Some(cancel_rx),
                                    &session,
                                )
                                .await
                                .map_err(|e| format!("Room call error: {}", e))
                            })
                            .await
                            .unwrap_or_else(Err);

                            match res {
                                Ok(()) => Ok(display_room),
                                Err(e) => Err(e),
                            }
                        },
                        Message::RoomResult,
                    ),
                ])
            }
            Message::HangupPressed => {
                if let Some(tx) = self.call_cancel_tx.take() {
                    let _ = tx.send(());
                }
                self.call_state = CallStatus::Idle;
                self.participants.clear();
                self.active_speakers.clear();
                self.input_volume = 0.0;
                self.output_volume = 0.0;
                self.status_info = "Call ended / hung up.".to_string();
                Task::none()
            }
            Message::ToggleMutePressed => {
                self.is_muted = !self.is_muted;
                if self.is_muted {
                    self.status_info = "Microphone muted".to_string();
                } else {
                    self.status_info = "Microphone unmuted".to_string();
                }
                Task::none()
            }
            Message::FetchAddressesPressed => {
                self.status_info = "Fetching registered user addresses from server...".to_string();
                let server_ip = self.config.server_ip;
                let server_port = self.config.server_port;

                Task::perform(
                    async move {
                        spawn_tokio(async move {
                            let server_url = match server_ip {
                                std::net::IpAddr::V4(ip) => {
                                    format!("http://{}:{}", ip, server_port)
                                }
                                std::net::IpAddr::V6(ip) => {
                                    format!("http://[{}]:{}", ip, server_port)
                                }
                            };
                            let endpoint = format!("{}/addresses", server_url);
                            let client = reqwest::Client::new();
                            match client.get(&endpoint).send().await {
                                Ok(resp) => {
                                    if resp.status().is_success() {
                                        match resp.json::<Vec<UserAddress>>().await {
                                            Ok(list) => Ok(list),
                                            Err(e) => {
                                                Err(format!("Failed to parse response: {}", e))
                                            }
                                        }
                                    } else {
                                        Err(format!(
                                            "Server returned status code: {}",
                                            resp.status()
                                        ))
                                    }
                                }
                                Err(e) => Err(format!("HTTP Request failed: {}", e)),
                            }
                        })
                        .await
                        .unwrap_or_else(Err)
                    },
                    Message::AddressesFetched,
                )
            }
            Message::AddressesFetched(res) => {
                match res {
                    Ok(list) => {
                        self.status_info = format!("Loaded {} online user addresses.", list.len());
                        self.registered_addresses = Some(list);
                    }
                    Err(err) => {
                        self.status_info = format!("Address fetch error: {}", err);
                    }
                }
                Task::none()
            }
            Message::FetchDevicesPressed => {
                self.status_info = "Enumerating audio hardware devices...".to_string();
                Task::perform(
                    async move {
                        spawn_tokio(async move {
                            wpapi::audio::get_device_names()
                                .map_err(|e| format!("Failed to fetch devices: {}", e))
                        })
                        .await
                        .unwrap_or_else(Err)
                    },
                    Message::DevicesFetched,
                )
            }
            Message::DevicesFetched(res) => {
                match res {
                    Ok((inputs, outputs)) => {
                        self.status_info = format!(
                            "Audio devices loaded: {} input, {} output.",
                            inputs.len(),
                            outputs.len()
                        );
                        self.input_devices = inputs;
                        self.output_devices = outputs;
                    }
                    Err(err) => {
                        self.status_info = format!("Device query error: {}", err);
                    }
                }
                Task::none()
            }
            Message::CallUserSelected(addr_str) => {
                self.target_address_input = addr_str.clone();
                self.status_info = format!("Selected target user address: {}", addr_str);
                Task::none()
            }
            Message::VolumeUpdated { input, output } => {
                if !self.is_muted {
                    self.input_volume = input;
                } else {
                    self.input_volume = 0.0;
                }
                self.output_volume = output;
                Task::none()
            }
            Message::ParticipantListUpdated(list) => {
                self.participants = list;
                Task::none()
            }
            Message::ActiveSpeakersUpdated(speakers) => {
                self.active_speakers = speakers;
                Task::none()
            }
            Message::CallResult(res) => {
                self.call_state = CallStatus::Idle;
                self.participants.clear();
                self.active_speakers.clear();
                self.input_volume = 0.0;
                self.output_volume = 0.0;
                match res {
                    Ok(_) => {
                        self.status_info = "1-to-1 WebRTC Call ended.".to_string();
                    }
                    Err(e) => {
                        self.status_info = format!("Call error: {}", e);
                    }
                }
                Task::none()
            }
            Message::RoomResult(res) => {
                self.call_state = CallStatus::Idle;
                self.participants.clear();
                self.active_speakers.clear();
                self.input_volume = 0.0;
                self.output_volume = 0.0;
                match res {
                    Ok(_) => {
                        self.status_info = "SFU Room call ended.".to_string();
                    }
                    Err(e) => {
                        self.status_info = format!("Room call error: {}", e);
                    }
                }
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let title_text = text("web-phone Desktop GUI").size(28);

        // 0. Incoming Call Banner (If any)
        let incoming_banner = if let Some(caller) = &self.incoming_call_from {
            column![
                text("🔔 INCOMING CALL REQUEST!").size(20),
                text(format!("Caller Short ID: {}", caller.short_id())).size(15),
                row![
                    button("Accept Call").on_press(Message::AcceptIncomingCallPressed),
                    button("Reject Call").on_press(Message::RejectIncomingCallPressed),
                ]
                .spacing(15)
            ]
            .spacing(10)
        } else {
            column![]
        };

        // 1. My Identity Card (Short ID & Full UserAddress)
        let auto_accept_label = if self.auto_accept {
            "Auto-Accept Incoming Calls: [ENABLED]"
        } else {
            "Auto-Accept Incoming Calls: [DISABLED]"
        };
        let auto_accept_btn =
            button(text(auto_accept_label)).on_press(Message::ToggleAutoAcceptPressed);

        let identity_card = column![
            text("Your Identity & Settings").size(18),
            row![
                text(format!("Short ID:  {}", self.my_address.short_id())).size(16),
                button("Generate New Short ID").on_press(Message::GenerateNewIdPressed),
                auto_accept_btn,
            ]
            .spacing(15)
            .align_y(Alignment::Center),
            text(format!("Full Address (SHA-256): {}", self.my_address.id)).size(12),
        ]
        .spacing(8);

        let status_badge = match &self.call_state {
            CallStatus::Idle => text("Status: Standby (Idle)").size(16),
            CallStatus::Connecting => text("Status: Connecting...").size(16),
            CallStatus::InCall { target } => {
                text(format!("Status: 1-to-1 Call Connected with {}", target)).size(16)
            }
            CallStatus::InRoom { room } => {
                text(format!("Status: SFU Group Room Connected ({})", room)).size(16)
            }
        };

        // 2. Direct Call Controls
        let call_controls = column![
            text("1-to-1 Direct Call").size(18),
            row![
                text_input(
                    "Target UserAddress or Short ID (Optional for receiving calls)",
                    &self.target_address_input
                )
                .on_input(Message::TargetAddressChanged)
                .width(Length::FillPortion(3)),
                button("Start Call / Standby")
                    .on_press(Message::StartCallPressed)
                    .width(Length::FillPortion(1)),
            ]
            .spacing(10)
        ]
        .spacing(6);

        // 3. Room Call Controls
        let room_controls = column![
            text("Group Audio Room (SFU - WPIP-08)").size(18),
            row![
                text_input(
                    "Enter Group Room UserAddress or Short ID",
                    &self.room_address_input
                )
                .on_input(Message::RoomAddressChanged)
                .width(Length::FillPortion(3)),
                button("Join Group Room")
                    .on_press(Message::StartRoomPressed)
                    .width(Length::FillPortion(1)),
            ]
            .spacing(10)
        ]
        .spacing(6);

        // 4. Audio Volume VU Meters & Mute Toggle
        let mute_button_label = if self.is_muted {
            "Unmute Microphone [Muted]"
        } else {
            "Mute Microphone"
        };
        let mute_btn = button(text(mute_button_label)).on_press(Message::ToggleMutePressed);

        let input_meter = row![
            text("Mic Input (VU): ").width(Length::Fixed(120.0)),
            progress_bar(0.0..=1.0, self.input_volume),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let output_meter = row![
            text("Speaker Output: ").width(Length::Fixed(120.0)),
            progress_bar(0.0..=1.0, self.output_volume),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let audio_section = column![
            text("Audio Controls & Real-Time VU Meters").size(18),
            mute_btn,
            input_meter,
            output_meter,
        ]
        .spacing(10);

        // 5. Structured Audio Devices UI Card
        let refresh_devices_btn =
            button("Scan Audio Hardware Devices").on_press(Message::FetchDevicesPressed);

        let mut input_dev_col = column![text("🎤 Microphone Input Devices:").size(15)];
        if self.input_devices.is_empty() {
            input_dev_col = input_dev_col.push(text("  - Default System Microphone"));
        } else {
            for (i, dev) in self.input_devices.iter().enumerate() {
                let is_default = if i == 0 { " [Default]" } else { "" };
                input_dev_col =
                    input_dev_col.push(text(format!("  • {}{}", dev, is_default)).size(14));
            }
        }

        let mut output_dev_col = column![text("🔊 Speaker Output Devices:").size(15)];
        if self.output_devices.is_empty() {
            output_dev_col = output_dev_col.push(text("  - Default System Output Speaker"));
        } else {
            for (i, dev) in self.output_devices.iter().enumerate() {
                let is_default = if i == 0 { " [Default]" } else { "" };
                output_dev_col =
                    output_dev_col.push(text(format!("  • {}{}", dev, is_default)).size(14));
            }
        }

        let devices_card = column![
            row![
                text("Audio Hardware Diagnostics").size(18),
                refresh_devices_btn
            ]
            .spacing(15)
            .align_y(Alignment::Center),
            row![
                column![input_dev_col].width(Length::FillPortion(1)),
                column![output_dev_col].width(Length::FillPortion(1)),
            ]
            .spacing(20),
        ]
        .spacing(10);

        // 6. Online Address Discovery UI Directory
        let fetch_addresses_btn =
            button("Fetch Online Users").on_press(Message::FetchAddressesPressed);

        let mut utils_section = column![
            row![
                text("Online Directory & Discovery").size(18),
                fetch_addresses_btn
            ]
            .spacing(15)
            .align_y(Alignment::Center)
        ]
        .spacing(10);

        if let Some(addresses) = &self.registered_addresses {
            let mut addr_col =
                column![text(format!("Registered Users Online ({}):", addresses.len())).size(15)];
            if addresses.is_empty() {
                addr_col = addr_col.push(text("No registered users online currently."));
            } else {
                for (idx, addr) in addresses.iter().enumerate() {
                    let addr_str = addr.to_string();
                    let addr_short = addr.short_id().to_string();
                    let item_row = row![
                        text(format!("{}. Short ID: {}", idx + 1, addr_short)).size(14),
                        button("Select for Call").on_press(Message::CallUserSelected(addr_str)),
                    ]
                    .spacing(15)
                    .align_y(Alignment::Center);
                    addr_col = addr_col.push(item_row);
                }
            }
            utils_section = utils_section.push(addr_col);
        }

        // 7. Room Participants & Active Speaker Indicators (WPIP-08)
        let mut participant_elements =
            column![text("Room Participants & Active Speakers (WPIP-08)").size(18)];
        if self.participants.is_empty() {
            participant_elements = participant_elements.push(text("No active room participants."));
        } else {
            for p in &self.participants {
                let is_speaking = self.active_speakers.contains(p);
                let label = if is_speaking {
                    format!("🔊 [Active Speaker] {}", p)
                } else {
                    format!("👤 {}", p)
                };
                participant_elements = participant_elements.push(text(label));
            }
        }

        // 8. Hangup Button & Status Footer
        let hangup_btn = button(text("End / Hangup Call"))
            .on_press(Message::HangupPressed)
            .width(Length::Fill);

        let status_footer = text(format!("Info: {}", self.status_info)).size(14);

        let content = column![
            title_text,
            incoming_banner,
            identity_card,
            vertical_space().height(5),
            status_badge,
            vertical_space().height(10),
            call_controls,
            vertical_space().height(10),
            room_controls,
            vertical_space().height(15),
            audio_section,
            vertical_space().height(15),
            devices_card,
            vertical_space().height(15),
            utils_section,
            vertical_space().height(15),
            participant_elements,
            vertical_space().height(15),
            hangup_btn,
            status_footer,
        ]
        .spacing(15)
        .padding(20);

        scrollable(container(content).width(Length::Fill)).into()
    }
}

/// Launch iced GUI for wpclient.
pub fn run_gui(config: Configuration) -> iced::Result {
    iced::application(
        "web-phone Desktop (wpclient)",
        WpClientGui::update,
        WpClientGui::view,
    )
    .theme(WpClientGui::theme)
    .run_with(move || WpClientGui::new(config))
}
