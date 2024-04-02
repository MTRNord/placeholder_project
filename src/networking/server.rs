use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;
use bevy::utils::Duration;

use lightyear::prelude::server::*;
use lightyear::prelude::*;

use crate::networking::shared_movement_behaviour;

use super::{protocol::*, shared_config, SharedSettings};

// Plugin group to add all server-related plugins
pub struct ServerPluginGroup {
    pub(crate) lightyear: ServerPlugin<MatrixRPGGameProto>,
}

impl ServerPluginGroup {
    pub(crate) fn new(
        transport_configs: Vec<TransportConfig>,
        shared_settings: SharedSettings,
    ) -> ServerPluginGroup {
        // Step 1: create the io (transport + link conditioner)
        let link_conditioner = LinkConditionerConfig {
            incoming_latency: Duration::from_millis(200),
            incoming_jitter: Duration::from_millis(20),
            incoming_loss: 0.05,
        };
        let mut net_configs = vec![];
        for transport_config in transport_configs {
            net_configs.push(NetConfig::Netcode {
                config: NetcodeConfig::default()
                    .with_protocol_id(shared_settings.protocol_id)
                    .with_key(shared_settings.private_key),
                io: IoConfig::from_transport(transport_config)
                    .with_conditioner(link_conditioner.clone()),
            });
        }

        // Step 2: define the server configuration
        let config = ServerConfig {
            shared: shared_config(),
            net: net_configs,
            ..default()
        };

        // Step 3: create the plugin
        let plugin_config = PluginConfig::new(config, protocol());
        ServerPluginGroup {
            lightyear: ServerPlugin::new(plugin_config),
        }
    }
}

impl PluginGroup for ServerPluginGroup {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(self.lightyear)
            .add(MatrixRPGServerPlugin)
            .add(super::SharedPlugin)
    }
}

// Plugin for server-specific logic
pub struct MatrixRPGServerPlugin;

impl Plugin for MatrixRPGServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init);
        // Re-adding Replicate components to client-replicated entities must be done in this set for proper handling.
        app.add_systems(
            PreUpdate,
            (replicate_players).in_set(MainSet::ClientReplication),
        );
        // the physics/FixedUpdates systems that consume inputs should be run in this set
        app.add_systems(FixedUpdate, movement);
        //app.add_systems(Update, send_message);
        app.add_systems(Update, handle_disconnections);
    }
}

pub(crate) fn init(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
    commands.spawn(TextBundle::from_section(
        "Server",
        TextStyle {
            font_size: 30.0,
            color: Color::WHITE,
            ..default()
        },
    ));
}

/// Server disconnection system, delete all player entities upon disconnection
pub(crate) fn handle_disconnections(
    mut disconnections: EventReader<DisconnectEvent>,
    mut commands: Commands,
    player_entities: Query<(Entity, &PlayerId)>,
) {
    for disconnection in disconnections.read() {
        let client_id = disconnection.context();
        for (entity, player_id) in player_entities.iter() {
            if player_id.0 == *client_id {
                commands.entity(entity).despawn();
            }
        }
    }
}

/// Read client inputs and move players
pub(crate) fn movement(
    mut position_query: Query<(&mut PlayerPosition, &PlayerId)>,
    mut input_reader: EventReader<InputEvent<Inputs>>,
    tick_manager: Res<TickManager>,
) {
    for input in input_reader.read() {
        let client_id = input.context();
        if let Some(input) = input.input() {
            debug!(
                "Receiving input: {:?} from client: {:?} on tick: {:?}",
                input,
                client_id,
                tick_manager.tick()
            );

            for (position, player_id) in position_query.iter_mut() {
                if player_id.0 == *client_id {
                    // NOTE: be careful to directly pass Mut<PlayerPosition>
                    // getting a mutable reference triggers change detection, unless you use `as_deref_mut()`
                    shared_movement_behaviour(position, input);
                }
            }
        }
    }
}

// // NOTE: you can use either:
// // - ServerMut (which is a wrapper around a bunch of resources used in lightyear)
// // - ResMut<ConnectionManager>, which is the actual resource used to send the message in this case. This is more optimized
// //   because it enables more parallelism
// /// Send messages from server to clients (only in non-headless mode, because otherwise we run with minimal plugins
// /// and cannot do input handling)
// pub(crate) fn send_message(
//     mut server: ResMut<ServerConnectionManager>,
//     input: Option<Res<ButtonInput<KeyCode>>>,
// ) {
//     if input.is_some_and(|input| input.pressed(KeyCode::KeyM)) {
//         let message = Message1(5);
//         info!("Send message: {:?}", message);
//         server
//             .send_message_to_target::<Channel1, Message1>(Message1(5), NetworkTarget::All)
//             .unwrap_or_else(|e| {
//                 error!("Failed to send message: {:?}", e);
//             });
//     }
// }

// Replicate the pre-spawned entities back to the client
// Note that this needs to run before FixedUpdate, since we handle client inputs in the FixedUpdate schedule (subject to change)
// And we want to handle deletion properly
pub(crate) fn replicate_players(
    mut commands: Commands,
    mut player_spawn_reader: EventReader<ComponentInsertEvent<PlayerPosition>>,
) {
    for event in player_spawn_reader.read() {
        debug!("received player spawn event: {:?}", event);
        let client_id = event.context();
        let entity = event.entity();

        // for all cursors we have received, add a Replicate component so that we can start replicating it
        // to other clients
        if let Some(mut e) = commands.get_entity(entity) {
            e.insert(Replicate {
                // we want to replicate back to the original client, since they are using a pre-spawned entity
                replication_target: NetworkTarget::All,
                // NOTE: even with a pre-spawned Predicted entity, we need to specify who will run prediction
                // NOTE: Be careful to not override the pre-spawned prediction! we do not need to enable prediction
                //  because there is a pre-spawned predicted entity
                prediction_target: NetworkTarget::Only(vec![*client_id]),
                // we want the other clients to apply interpolation for the player
                interpolation_target: NetworkTarget::AllExcept(vec![*client_id]),
                ..default()
            });
        }
    }
}
