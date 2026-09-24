use crate::outgoing_message::ConnectionId;
use codex_hooks::HookOwnerChangePolicy;

pub(super) fn change_policy(
    subscribed_connection_ids: &[ConnectionId],
    requesting_connection_id: ConnectionId,
) -> HookOwnerChangePolicy {
    if subscribed_connection_ids
        .iter()
        .any(|connection_id| *connection_id != requesting_connection_id)
    {
        HookOwnerChangePolicy::RequireSameOwner
    } else {
        HookOwnerChangePolicy::AllowDifferentOwner
    }
}
