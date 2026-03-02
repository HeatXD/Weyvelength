use std::net::SocketAddr;
use std::sync::Arc;

use tokio_stream::wrappers::UnboundedReceiverStream;
use tonic::{Request, Response, Status, transport::Server};

use crate::codegen::weyvelength::{
    ChatMessage, CreateSessionRequest, CreateSessionResponse, GetMembersRequest,
    GetMembersResponse, GetServerInfoRequest, GetServerInfoResponse, GlobalMembersEvent, IceServer,
    JoinSessionRequest, JoinSessionResponse, LeaveSessionRequest, LeaveSessionResponse,
    ListSessionsRequest, ListSessionsResponse, LoginRequest, LoginResponse, RegisterRequest,
    RegisterResponse, SendMessageRequest, SendMessageResponse, SendSignalRequest,
    SendSignalResponse, SessionsUpdatedEvent, Signal, StartGameRequest, StartGameResponse,
    StopGameRequest, StopGameResponse, StreamGlobalMembersRequest, StreamMessagesRequest,
    StreamSessionUpdatesRequest, StreamSignalsRequest,
    weyvelength_server::{Weyvelength, WeyvelengthServer},
};
use crate::state::{IceServerConfig, ServerState, SharedState};

impl From<&IceServerConfig> for IceServer {
    fn from(s: &IceServerConfig) -> Self {
        IceServer {
            url: s.url.clone(),
            username: s.username.clone(),
            credential: s.credential.clone(),
            name: s.name.clone(),
        }
    }
}

mod auth;
mod helpers;
mod messaging;
mod sessions;
mod signals;
mod streams;

// ── service struct ────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct WeyvelengthService {
    state: SharedState,
}

#[tonic::async_trait]
impl Weyvelength for WeyvelengthService {
    // ── auth ─────────────────────────────────────────────────────────────────

    async fn register(
        &self,
        request: Request<RegisterRequest>,
    ) -> Result<Response<RegisterResponse>, Status> {
        auth::handle_register(&self.state, request.into_inner()).await
    }

    async fn login(
        &self,
        request: Request<LoginRequest>,
    ) -> Result<Response<LoginResponse>, Status> {
        auth::handle_login(&self.state, request.into_inner()).await
    }

    // ── server info ──────────────────────────────────────────────────────────

    async fn get_server_info(
        &self,
        _request: Request<GetServerInfoRequest>,
    ) -> Result<Response<GetServerInfoResponse>, Status> {
        let state = &self.state;
        let ice_servers = state.ice_servers.iter().map(IceServer::from).collect();
        Ok(Response::new(GetServerInfoResponse {
            server_name: state.server_name.clone(),
            motd: state.motd.clone(),
            ice_servers,
        }))
    }

    // ── sessions ─────────────────────────────────────────────────────────────

    async fn list_sessions(
        &self,
        request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        sessions::handle_list_sessions(&self.state, request.into_inner()).await
    }

    async fn create_session(
        &self,
        request: Request<CreateSessionRequest>,
    ) -> Result<Response<CreateSessionResponse>, Status> {
        let authed = auth::validate_token(&self.state.db, request.metadata()).await?;
        let req = request.into_inner();
        if authed != req.username {
            return Err(Status::permission_denied("Token does not match username"));
        }
        sessions::handle_create_session(&self.state, req).await
    }

    async fn join_session(
        &self,
        request: Request<JoinSessionRequest>,
    ) -> Result<Response<JoinSessionResponse>, Status> {
        let authed = auth::validate_token(&self.state.db, request.metadata()).await?;
        let req = request.into_inner();
        if authed != req.username {
            return Err(Status::permission_denied("Token does not match username"));
        }
        sessions::handle_join_session(&self.state, req).await
    }

    async fn leave_session(
        &self,
        request: Request<LeaveSessionRequest>,
    ) -> Result<Response<LeaveSessionResponse>, Status> {
        let authed = auth::validate_token(&self.state.db, request.metadata()).await?;
        let req = request.into_inner();
        if authed != req.username {
            return Err(Status::permission_denied("Token does not match username"));
        }
        sessions::handle_leave_session(&self.state, req).await
    }

    async fn start_game(
        &self,
        request: Request<StartGameRequest>,
    ) -> Result<Response<StartGameResponse>, Status> {
        let authed = auth::validate_token(&self.state.db, request.metadata()).await?;
        let req = request.into_inner();
        if authed != req.username {
            return Err(Status::permission_denied("Token does not match username"));
        }
        sessions::handle_start_game(&self.state, req).await
    }

    async fn stop_game(
        &self,
        request: Request<StopGameRequest>,
    ) -> Result<Response<StopGameResponse>, Status> {
        let authed = auth::validate_token(&self.state.db, request.metadata()).await?;
        let req = request.into_inner();
        if authed != req.username {
            return Err(Status::permission_denied("Token does not match username"));
        }
        sessions::handle_stop_game(&self.state, req).await
    }

    // ── messaging ────────────────────────────────────────────────────────────

    async fn send_message(
        &self,
        request: Request<SendMessageRequest>,
    ) -> Result<Response<SendMessageResponse>, Status> {
        let authed = auth::validate_token(&self.state.db, request.metadata()).await?;
        let req = request.into_inner();
        if authed != req.username {
            return Err(Status::permission_denied("Token does not match username"));
        }
        messaging::handle_send_message(&self.state, req).await
    }

    async fn get_members(
        &self,
        request: Request<GetMembersRequest>,
    ) -> Result<Response<GetMembersResponse>, Status> {
        auth::validate_token(&self.state.db, request.metadata()).await?;
        sessions::handle_get_members(&self.state, request.into_inner()).await
    }

    // ── signaling ────────────────────────────────────────────────────────────

    async fn send_signal(
        &self,
        request: Request<SendSignalRequest>,
    ) -> Result<Response<SendSignalResponse>, Status> {
        let authed = auth::validate_token(&self.state.db, request.metadata()).await?;
        let req = request.into_inner();
        let from = req.signal.as_ref().map(|s| s.from_user.as_str()).unwrap_or("");
        if authed != from {
            return Err(Status::permission_denied("Token does not match username"));
        }
        signals::handle_send_signal(&self.state, req).await
    }

    // ── streaming ────────────────────────────────────────────────────────────

    type StreamMessagesStream = UnboundedReceiverStream<Result<ChatMessage, Status>>;

    async fn stream_messages(
        &self,
        request: Request<StreamMessagesRequest>,
    ) -> Result<Response<Self::StreamMessagesStream>, Status> {
        let authed = auth::validate_token(&self.state.db, request.metadata()).await?;
        let req = request.into_inner();
        if authed != req.username {
            return Err(Status::permission_denied("Token does not match username"));
        }
        streams::handle_stream_messages(&self.state, req).await
    }

    type StreamSignalsStream = UnboundedReceiverStream<Result<Signal, Status>>;

    async fn stream_signals(
        &self,
        request: Request<StreamSignalsRequest>,
    ) -> Result<Response<Self::StreamSignalsStream>, Status> {
        let authed = auth::validate_token(&self.state.db, request.metadata()).await?;
        let req = request.into_inner();
        if authed != req.username {
            return Err(Status::permission_denied("Token does not match username"));
        }
        signals::handle_stream_signals(&self.state, req).await
    }

    type StreamSessionUpdatesStream = UnboundedReceiverStream<Result<SessionsUpdatedEvent, Status>>;

    async fn stream_session_updates(
        &self,
        _request: Request<StreamSessionUpdatesRequest>,
    ) -> Result<Response<Self::StreamSessionUpdatesStream>, Status> {
        streams::handle_stream_session_updates(&self.state, StreamSessionUpdatesRequest {}).await
    }

    type StreamGlobalMembersStream = UnboundedReceiverStream<Result<GlobalMembersEvent, Status>>;

    async fn stream_global_members(
        &self,
        _request: Request<StreamGlobalMembersRequest>,
    ) -> Result<Response<Self::StreamGlobalMembersStream>, Status> {
        streams::handle_stream_global_members(&self.state, StreamGlobalMembersRequest {}).await
    }
}

// ── entry point ──────────────────────────────────────────────────────────────

pub async fn run(
    addr: SocketAddr,
    server_name: String,
    motd: String,
    ice_servers: Vec<IceServerConfig>,
    db: sqlx::SqlitePool,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = Arc::new(ServerState::new(server_name, motd, ice_servers, db));
    let service = WeyvelengthService { state };

    println!("Weyvelength gRPC server listening on {addr}");

    Server::builder()
        .add_service(WeyvelengthServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
