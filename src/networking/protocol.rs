use derive_more::{Add, Mul};

use bevy::prelude::*;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct Direction {
    pub(crate) up: bool,
    pub(crate) down: bool,
    pub(crate) left: bool,
    pub(crate) right: bool,
}

impl Direction {
    pub(crate) fn is_none(&self) -> bool {
        !self.up && !self.down && !self.left && !self.right
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub enum Inputs {
    Direction(Direction),
    Delete,
    Spawn,
    // NOTE: we NEED to provide a None input so that the server can distinguish between lost input packets and 'None' inputs
    None,
}
impl UserAction for Inputs {}

#[derive(Message, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Message1(pub usize);

#[message_protocol(protocol = "MatrixRPGGameProto")]
pub enum Messages {
    Message1(Message1),
}

#[derive(Component, Message, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerId(pub ClientId);

// `Deref` and `DerefMut` are from bevy
// `Add` and `Mul` are from the derive_more crate
#[derive(
    Component, Message, Serialize, Deserialize, Clone, Debug, PartialEq, Deref, DerefMut, Add, Mul,
)]
pub struct PlayerPosition(pub Vec2);

impl std::ops::Mul<f32> for &PlayerPosition {
    type Output = PlayerPosition;

    fn mul(self, rhs: f32) -> Self::Output {
        PlayerPosition(self.0 * rhs)
    }
}

#[component_protocol(protocol = "MatrixRPGGameProto")]
pub enum Components {
    #[sync(once)]
    PlayerId(PlayerId),
    #[sync(full)]
    PlayerPosition(PlayerPosition),
}

#[derive(Channel)]
pub struct Channel1;

protocolize! {
    Self = MatrixRPGGameProto,
    Message = Messages,
    Component = Components,
    Input = Inputs,
}

pub(crate) fn protocol() -> MatrixRPGGameProto {
    let mut protocol = MatrixRPGGameProto::default();
    protocol.add_channel::<Channel1>(ChannelSettings {
        mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
        ..default()
    });
    protocol
}
