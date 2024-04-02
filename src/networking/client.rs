use std::net::SocketAddr;

use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;
use bevy::utils::Duration;

use bevy_ecs_ldtk::LdtkWorldBundle;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

use crate::player::{AnimationIndices, AnimationTimer, PlayerBundle};

use super::protocol::{
    protocol, ClientMut, Components, Inputs, MatrixRPGGameProto, PlayerId, PlayerPosition,
};
use super::{shared_config, shared_movement_behaviour, SharedSettings};

pub struct ClientPluginGroup {
    lightyear: ClientPlugin<MatrixRPGGameProto>,
}

impl ClientPluginGroup {
    pub(crate) fn new(
        client_id: u64,
        server_addr: SocketAddr,
        transport_config: TransportConfig,
        shared_settings: SharedSettings,
    ) -> ClientPluginGroup {
        let auth = Authentication::Manual {
            server_addr,
            client_id,
            private_key: shared_settings.private_key,
            protocol_id: shared_settings.protocol_id,
        };
        let link_conditioner = LinkConditionerConfig {
            incoming_latency: Duration::from_millis(200),
            incoming_jitter: Duration::from_millis(20),
            incoming_loss: 0.05,
        };
        let config = ClientConfig {
            shared: shared_config(),
            net: NetConfig::Netcode {
                auth,
                config: NetcodeConfig::default(),
                io: IoConfig::from_transport(transport_config).with_conditioner(link_conditioner),
            },
            interpolation: InterpolationConfig {
                delay: InterpolationDelay::default(),
                custom_interpolation_logic: false,
            },
            ..default()
        };
        let plugin_config = PluginConfig::new(config, protocol());
        ClientPluginGroup {
            lightyear: ClientPlugin::new(plugin_config),
        }
    }
}

impl PluginGroup for ClientPluginGroup {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(self.lightyear)
            .add(MatrixRPGClientPlugin)
            .add(super::SharedPlugin)
    }
}

pub struct MatrixRPGClientPlugin;

impl Plugin for MatrixRPGClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init);
        app.add_systems(PreUpdate, handle_connection.after(MainSet::ReceiveFlush));
        // Inputs have to be buffered in the FixedPreUpdate schedule
        app.add_systems(
            FixedPreUpdate,
            buffer_input.in_set(InputSystemSet::BufferInputs),
        );
        app.add_systems(FixedUpdate, player_movement);
        app.add_systems(Update, spawn_player);
    }
}

// Startup system for the client
pub(crate) fn init(mut commands: Commands, mut client: ClientMut, asset_server: Res<AssetServer>) {
    let mut camera = Camera2dBundle::default();
    camera.projection.scale = 0.5;
    camera.transform.translation.x += 1920.0 / 4.0;
    camera.transform.translation.y += 1080.0 / 4.0;
    commands.spawn(camera);

    commands.spawn(LdtkWorldBundle {
        ldtk_handle: asset_server.load("matrix_office.ldtk"),
        ..Default::default()
    });

    let _ = client.connect();
}

pub(crate) fn handle_connection(mut commands: Commands, metadata: Res<GlobalMetadata>) {
    // the `GlobalMetadata` resource holds metadata related to the client
    // once the connection is established.
    if metadata.is_changed() {
        if let Some(client_id) = metadata.client_id {
            commands.spawn(TextBundle::from_section(
                format!("Client {}", client_id),
                TextStyle {
                    font_size: 30.0,
                    color: Color::WHITE,
                    ..default()
                },
            ));
        }
    }
}

// System that reads from peripherals and adds inputs to the buffer
pub(crate) fn buffer_input(mut client: ClientMut, keypress: Res<ButtonInput<KeyCode>>) {
    let mut direction = super::protocol::Direction {
        up: false,
        down: false,
        left: false,
        right: false,
    };
    if keypress.pressed(KeyCode::KeyW) || keypress.pressed(KeyCode::ArrowUp) {
        direction.up = true;
    }
    if keypress.pressed(KeyCode::KeyS) || keypress.pressed(KeyCode::ArrowDown) {
        direction.down = true;
    }
    if keypress.pressed(KeyCode::KeyA) || keypress.pressed(KeyCode::ArrowLeft) {
        direction.left = true;
    }
    if keypress.pressed(KeyCode::KeyD) || keypress.pressed(KeyCode::ArrowRight) {
        direction.right = true;
    }
    if !direction.is_none() {
        return client.add_input(Inputs::Direction(direction));
    }
    if keypress.pressed(KeyCode::Space) {
        return client.add_input(Inputs::Spawn);
    }
    // info!("Sending input: {:?} on tick: {:?}", &input, client.tick());
    client.add_input(Inputs::None)
}

// The client input only gets applied to predicted entities that we own
// This works because we only predict the user's controlled entity.
// If we were predicting more entities, we would have to only apply movement to the player owned one.
#[allow(clippy::type_complexity)]
fn player_movement(
    mut position_query: Query<
        (&mut Transform, &mut PlayerPosition),
        (With<Predicted>, With<PlayerId>, Without<Camera>),
    >,
    mut cameras: Query<&mut Transform, With<Camera>>,
    mut input_reader: EventReader<InputEvent<Inputs>>,
) {
    if <Components as SyncMetadata<PlayerPosition>>::mode() != ComponentSyncMode::Full {
        return;
    }
    for input in input_reader.read() {
        if let Some(input) = input.input() {
            for (mut transform, position) in position_query.iter_mut() {
                // NOTE: be careful to directly pass Mut<PlayerPosition>
                // getting a mutable reference triggers change detection, unless you use `as_deref_mut()`
                transform.translation = Vec3::new(position.x, position.y, transform.translation.z);
                let pos = transform.translation;
                for mut transform in &mut cameras {
                    transform.translation.x = pos.x;
                    transform.translation.y = pos.y;
                }
                shared_movement_behaviour(position, input);
            }
        }
    }
}

/// Spawn a player when the space command is pressed
fn spawn_player(
    mut commands: Commands,
    players: Query<&PlayerId, With<PlayerPosition>>,
    metadata: Res<GlobalMetadata>,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    // return early if we still don't have access to the client id
    let Some(client_id) = metadata.client_id else {
        return;
    };

    for player_id in players.iter() {
        if player_id.0 == client_id {
            return;
        }
    }
    info!("got spawn input");

    let texture = asset_server.load("tilesets/user.png");
    let layout = TextureAtlasLayout::from_grid(Vec2::new(16.0, 16.0), 8, 8, None, None);
    let texture_atlas_layout = texture_atlas_layouts.add(layout);
    // Use only the subset of sprites in the sheet that make up the run animation
    let animation_indices = AnimationIndices { first: 0, last: 3 };
    let atlas = TextureAtlas {
        layout: texture_atlas_layout.clone(),
        index: animation_indices.first,
    };
    commands.spawn((
        PlayerBundle::new(client_id, Vec2::ZERO),
        AnimationTimer(Timer::from_seconds(0.3, TimerMode::Repeating)),
        animation_indices,
        SpriteSheetBundle {
            transform: Transform::from_xyz(0., 0., 17.).with_scale(Vec3::splat(2.0)),
            texture: texture.clone(),
            atlas,
            ..default()
        },
        // IMPORTANT: this lets the server know that the entity is pre-predicted
        // when the server replicates this entity; we will get a Confirmed entity which will use this entity
        // as the Predicted version
        ShouldBePredicted::default(),
    ));
}
