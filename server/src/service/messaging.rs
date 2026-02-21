use tonic::{Response, Status};

use crate::codegen::weyvelength::{
    ChatMessage, SendMessageRequest, SendMessageResponse,
};
use crate::session::now_timestamp;
use crate::state::SharedState;

pub async fn handle_send_message(
    state: &SharedState,
    req: SendMessageRequest,
) -> Result<Response<SendMessageResponse>, Status> {
    let state = state.lock().unwrap();

    let session = state
        .sessions
        .get(&req.session_id)
        .ok_or_else(|| Status::not_found("Session not found"))?;

    let msg = ChatMessage {
        username: req.username.clone(),
        content: req.content.clone(),
        timestamp: now_timestamp(),
    };
    println!("[msg] {} in {}: {}", req.username, req.session_id, req.content);
    let _ = session.tx.send(msg);

    Ok(Response::new(SendMessageResponse {}))
}
