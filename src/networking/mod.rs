use std::net::Ipv4Addr;

use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::utils::Duration;

use lightyear::{client::components::Confirmed, prelude::*};
use serde::{Deserialize, Serialize};

use self::protocol::{Inputs, PlayerPosition};

pub mod client;
pub mod protocol;
#[cfg(not(target_family = "wasm"))]
pub mod server;

pub fn shared_config() -> SharedConfig {
    SharedConfig {
        client_send_interval: Duration::default(),
        server_send_interval: Duration::from_millis(40),
        // server_send_interval: Duration::from_millis(100),
        tick: TickConfig {
            tick_duration: Duration::from_secs_f64(1.0 / 64.0),
        },
    }
}

pub struct SharedPlugin;

impl Plugin for SharedPlugin {
    fn build(&self, app: &mut App) {
        if app.is_plugin_added::<RenderPlugin>() {
            app.add_systems(PostUpdate, draw_elements);
            // app.add_plugins(LogDiagnosticsPlugin {
            //     filter: Some(vec![
            //         IoDiagnosticsPlugin::BYTES_IN,
            //         IoDiagnosticsPlugin::BYTES_OUT,
            //     ]),
            //     ..default()
            // });
        }
    }
}

// This system defines how we update the player's positions when we receive an input
pub(crate) fn shared_movement_behaviour(mut position: Mut<PlayerPosition>, input: &Inputs) {
    const MOVE_SPEED: f32 = 10.0;
    if let Inputs::Direction(direction) = input {
        if direction.up {
            position.y += MOVE_SPEED;
        }
        if direction.down {
            position.y -= MOVE_SPEED;
        }
        if direction.left {
            position.x -= MOVE_SPEED;
        }
        if direction.right {
            position.x += MOVE_SPEED;
        }
    }
}

/// System that draws the player's boxes and cursors
pub fn draw_elements(mut gizmos: Gizmos, players: Query<&PlayerPosition, Without<Confirmed>>) {
    for position in &players {
        gizmos.rect_2d(
            Vec2::new(position.x, position.y),
            0.0,
            Vec2::ONE * 40.0,
            Color::GREEN,
        );
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ClientTransports {
    #[cfg(not(target_family = "wasm"))]
    Udp,
    WebSocket,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ServerTransports {
    Udp { local_port: u16 },
    WebSocket { local_port: u16 },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ServerSettings {
    /// If true, disable any rendering-related plugins
    pub headless: bool,

    /// If true, enable bevy_inspector_egui
    pub inspector: bool,

    /// Which transport to use
    pub transport: Vec<ServerTransports>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ClientSettings {
    /// If true, enable bevy_inspector_egui
    pub inspector: bool,

    /// The client id
    pub client_id: u64,

    /// The client port to listen on
    pub client_port: u16,

    /// The ip address of the server
    pub server_addr: Ipv4Addr,

    /// The port of the server
    pub server_port: u16,

    /// Which transport to use
    pub transport: ClientTransports,
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub struct SharedSettings {
    /// An id to identify the protocol version
    pub protocol_id: u64,

    /// a 32-byte array to authenticate via the Netcode.io protocol
    pub private_key: [u8; 32],
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Settings {
    pub server: ServerSettings,
    pub client: ClientSettings,
    pub shared: SharedSettings,
}
