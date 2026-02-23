use tonic::{Response, Status};

use crate::codegen::weyvelength::{ChatMessage, SendMessageRequest, SendMessageResponse};
use crate::session::now_timestamp;
use crate::state::SharedState;

pub async fn handle_send_message(
    state: &SharedState,
    req: SendMessageRequest,
) -> Result<Response<SendMessageResponse>, Status> {
    // Clone the broadcast Sender out of the DashMap entry.
    // The shard lock is held only for the duration of the clone — zero async waits.
    let tx = {
        let entry = state
            .sessions
            .get(&req.session_id)
            .ok_or_else(|| Status::not_found("Session not found"))?;
        entry.tx.clone()
    };

    let msg = ChatMessage {
        username: req.username.clone(),
        content: req.content.clone(),
        timestamp: now_timestamp(),
    };
    println!(
        "[msg] {} in {}: {}",
        req.username, req.session_id, req.content
    );
    let _ = tx.send(msg);

    Ok(Response::new(SendMessageResponse {}))
}
