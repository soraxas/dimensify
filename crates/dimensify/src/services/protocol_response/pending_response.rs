use bevy::prelude::*;
use dimensify_transport::ProtoResponse;
use lightyear::prelude::MessageSender;

/// A populated type for entities that are waiting for a response.
pub type WithPendingResponse<'a, 'b, 'c, F> =
    Populated<'a, 'b, (Entity, &'c mut MessageSender<ProtoResponse>), F>;

/// A marker component for sender that are waiting for a list request.
#[derive(Component)]
pub(crate) struct PendingRequestList;

/// A marker component for sender that are waiting for a response from an entity command.
#[derive(Component)]
pub(crate) struct PendingApplyCommand;
