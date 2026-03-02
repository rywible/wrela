use crate::game_slice::{self, VerticalSliceServerConfig};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionAuthorityRequest {
    pub partition_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionGrant {
    pub partition_key: String,
    pub authority_token: String,
}

pub trait PartitionAuthority {
    fn authorize_partition(
        &self,
        request: &PartitionAuthorityRequest,
    ) -> Result<PartitionGrant, String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionRequest {
    pub topic: String,
    pub subscriber_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionRoute {
    pub stream_key: String,
    pub partition_key: String,
}

pub trait SubscriptionResolver {
    fn resolve_subscription(
        &self,
        request: &SubscriptionRequest,
    ) -> Result<SubscriptionRoute, String>;
}

pub trait DeltaCodec {
    fn encode_delta(&self, delta: &JsonValue) -> Result<Vec<u8>, String>;
    fn decode_delta(&self, bytes: &[u8]) -> Result<JsonValue, String>;
}

#[derive(Debug, Clone, Default)]
pub struct GameSlicePartitionAuthority;

impl PartitionAuthority for GameSlicePartitionAuthority {
    fn authorize_partition(
        &self,
        request: &PartitionAuthorityRequest,
    ) -> Result<PartitionGrant, String> {
        Ok(PartitionGrant {
            partition_key: request.partition_key.clone(),
            authority_token: format!("game-slice:{}", request.partition_key),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct GameSliceSubscriptionResolver;

impl SubscriptionResolver for GameSliceSubscriptionResolver {
    fn resolve_subscription(
        &self,
        request: &SubscriptionRequest,
    ) -> Result<SubscriptionRoute, String> {
        let partition_key = format!("partition/{}", request.topic);
        Ok(SubscriptionRoute {
            stream_key: format!("{partition_key}/subscriber/{}", request.subscriber_id),
            partition_key,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct GameSliceDeltaCodec;

impl DeltaCodec for GameSliceDeltaCodec {
    fn encode_delta(&self, delta: &JsonValue) -> Result<Vec<u8>, String> {
        serde_json::to_vec(delta)
            .map_err(|error| format!("failed to encode realtime delta: {error}"))
    }

    fn decode_delta(&self, bytes: &[u8]) -> Result<JsonValue, String> {
        serde_json::from_slice(bytes)
            .map_err(|error| format!("failed to decode realtime delta: {error}"))
    }
}

#[derive(Debug, Clone)]
pub struct RealtimeKernel<A, S, C> {
    pub partition_authority: A,
    pub subscription_resolver: S,
    pub delta_codec: C,
}

impl<A, S, C> RealtimeKernel<A, S, C> {
    pub const fn new(partition_authority: A, subscription_resolver: S, delta_codec: C) -> Self {
        Self {
            partition_authority,
            subscription_resolver,
            delta_codec,
        }
    }
}

pub type DefaultRealtimeKernel =
    RealtimeKernel<GameSlicePartitionAuthority, GameSliceSubscriptionResolver, GameSliceDeltaCodec>;

impl Default for DefaultRealtimeKernel {
    fn default() -> Self {
        Self::new(
            GameSlicePartitionAuthority,
            GameSliceSubscriptionResolver,
            GameSliceDeltaCodec,
        )
    }
}

impl<A, S, C> RealtimeKernel<A, S, C>
where
    A: PartitionAuthority,
    S: SubscriptionResolver,
    C: DeltaCodec,
{
    pub fn run_vertical_slice_server(
        &self,
        config: VerticalSliceServerConfig,
    ) -> Result<(), String> {
        game_slice::run_vertical_slice_server(config)
    }
}

pub fn run_default_realtime_vertical_slice_server(
    config: VerticalSliceServerConfig,
) -> Result<(), String> {
    DefaultRealtimeKernel::default().run_vertical_slice_server(config)
}
