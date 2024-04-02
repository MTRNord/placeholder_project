use std::net::{Ipv4Addr, SocketAddr};

use bevy::{
    log::{Level, LogPlugin},
    prelude::*,
    scene::ron,
    window::PresentMode,
};
use bevy_ecs_ldtk::prelude::*;
use clap::Parser;
use iyes_perf_ui::{PerfUiCompleteBundle, PerfUiPlugin};
use lightyear::{
    connection::netcode::ClientId,
    shared::log::add_log_layer,
    transport::{io::TransportConfig, LOCAL_SOCKET},
};
use networking::{
    client::ClientPluginGroup, server::ServerPluginGroup, ClientSettings, ClientTransports,
    ServerTransports, Settings,
};
use wall::WallBundle;

mod networking;
mod player;
mod wall;

#[derive(Parser, PartialEq, Debug)]
enum Cli {
    #[cfg(not(target_family = "wasm"))]
    /// The program will act both as a server and as a client
    ListenServer,
    #[cfg(not(target_family = "wasm"))]
    /// Dedicated server
    Server,
    /// The program will act as a client
    Client,
}

fn main() {
    cfg_if::cfg_if! {
        if #[cfg(target_family = "wasm")] {
            let client_id = rand::random::<u64>();
            let cli = Cli::Client {
                client_id: Some(client_id)
            };
        } else {
            let cli = Cli::parse();
        }
    }
    let settings_str = include_str!("../assets/settings.ron");
    let settings = ron::de::from_str::<Settings>(settings_str).unwrap();
    run(settings, cli);
}

fn run(settings: Settings, cli: Cli) {
    match cli {
        #[cfg(not(target_family = "wasm"))]
        Cli::ListenServer => {
            // create client app
            let (from_server_send, from_server_recv) = crossbeam_channel::unbounded();
            let (to_server_send, to_server_recv) = crossbeam_channel::unbounded();
            let transport_config = TransportConfig::LocalChannel {
                recv: from_server_recv,
                send: to_server_send,
            };
            // when communicating via channels, we need to use the address `LOCAL_SOCKET` for the server
            let client_id = rand::random::<u64>();
            let mut client_app =
                client_app(settings.clone(), LOCAL_SOCKET, client_id, transport_config);

            // create server app
            let extra_transport_configs = vec![TransportConfig::Channels {
                // even if we communicate via channels, we need to provide a socket address for the client
                channels: vec![(LOCAL_SOCKET, to_server_recv, from_server_send)],
            }];
            let mut server_app = server_app(settings, extra_transport_configs);

            // run both the client and server apps
            std::thread::spawn(move || server_app.run());
            client_app.run();
        }
        #[cfg(not(target_family = "wasm"))]
        Cli::Server => {
            let mut app = server_app(settings, vec![]);
            app.run();
        }
        Cli::Client => {
            let server_addr = SocketAddr::new(
                settings.client.server_addr.into(),
                settings.client.server_port,
            );
            let transport_config = get_client_transport_config(settings.client.clone());
            let client_id = rand::random::<u64>();
            let mut app = client_app(settings, server_addr, client_id, transport_config);
            app.run();
        }
    }
}

/// Build the client app
fn client_app(
    settings: Settings,
    server_addr: SocketAddr,
    client_id: ClientId,
    transport_config: TransportConfig,
) -> App {
    let mut app = App::new();
    // NOTE: create the default plugins first so that the async task pools are initialized
    // use the default bevy logger for now
    // (the lightyear logger doesn't handle wasm)
    app.add_plugins(
        DefaultPlugins
            .build()
            .set(LogPlugin {
                level: Level::INFO,
                filter: "wgpu=error,bevy_render=info".to_string(),
                update_subscriber: Some(add_log_layer),
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Matrix RPG".into(),
                    present_mode: PresentMode::AutoVsync,
                    prevent_default_event_handling: false,
                    ..default()
                }),
                ..default()
            })
            .set(ImagePlugin::default_nearest()),
    );
    if settings.client.inspector {
        app.add_plugins(PerfUiPlugin);
    }
    app.add_plugins(LdtkPlugin)
        .insert_resource(LevelSelection::index(0))
        .insert_resource(LdtkSettings {
            set_clear_color: SetClearColor::FromLevelBackground,
            ..Default::default()
        })
        .add_systems(Startup, move |mut commands: Commands| {
            if settings.client.inspector {
                commands.spawn(PerfUiCompleteBundle::default());
            }
        })
        .register_ldtk_int_cell::<WallBundle>(1)
        .add_plugins(player::PlayerPlugin);
    let client_plugin_group = ClientPluginGroup::new(
        // use the cli-provided client id if it exists, otherwise use the settings client id
        client_id,
        server_addr,
        transport_config,
        settings.shared,
    );
    app.add_plugins(client_plugin_group.build());
    app
}

/// Build the server app
fn server_app(settings: Settings, extra_transport_configs: Vec<TransportConfig>) -> App {
    let mut app = App::new();
    if !settings.server.headless {
        app.add_plugins(DefaultPlugins.build().disable::<LogPlugin>());
    } else {
        app.add_plugins(MinimalPlugins);
    }
    app.add_plugins(LogPlugin {
        level: Level::INFO,
        filter: "wgpu=error,bevy_render=info".to_string(),
        update_subscriber: Some(add_log_layer),
    });

    if settings.server.inspector {
        app.add_plugins(PerfUiPlugin);
    }
    app.add_systems(Startup, move |mut commands: Commands| {
        if settings.client.inspector {
            commands.spawn(PerfUiCompleteBundle::default());
        }
    });
    let mut transport_configs = get_server_transport_configs(settings.server.transport);
    transport_configs.extend(extra_transport_configs);
    let server_plugin_group = ServerPluginGroup::new(transport_configs, settings.shared);
    app.add_plugins(server_plugin_group.build());
    app
}

/// Parse the server transport settings into a list of `TransportConfig` that are used to configure the lightyear server
fn get_server_transport_configs(settings: Vec<ServerTransports>) -> Vec<TransportConfig> {
    settings
        .iter()
        .map(|t| match t {
            ServerTransports::Udp { local_port } => TransportConfig::UdpSocket(SocketAddr::new(
                Ipv4Addr::UNSPECIFIED.into(),
                *local_port,
            )),
            ServerTransports::WebSocket { local_port } => TransportConfig::WebSocketServer {
                server_addr: SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), *local_port),
            },
        })
        .collect()
}

/// Parse the client transport settings into a `TransportConfig` that is used to configure the lightyear client
fn get_client_transport_config(settings: ClientSettings) -> TransportConfig {
    let server_addr = SocketAddr::new(settings.server_addr.into(), settings.server_port);
    let client_addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), settings.client_port);
    match settings.transport {
        #[cfg(not(target_family = "wasm"))]
        ClientTransports::Udp => TransportConfig::UdpSocket(client_addr),
        ClientTransports::WebSocket => TransportConfig::WebSocketClient { server_addr },
    }
}
