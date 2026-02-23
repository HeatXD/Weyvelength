export interface SavedServer {
  id: string;
  displayName: string;
  host: string;
  port: number;
  username: string;
}

export interface ChatMessage {
  username: string;
  content: string;
  timestamp: number;
  /** True for join/leave notifications — rendered differently, never sent over the wire */
  system?: boolean;
}

export interface SessionInfo {
  id: string;
  name: string;
  member_count: number;
  is_public: boolean;
  max_members: number;
}

export interface SessionPayload {
  session_id: string;
  session_name: string;
  is_public: boolean;
  max_members: number;
  existing_peers: string[];
  host: string;
}

export interface IceServer {
  url: string;
  username: string;
  credential: string;
  name: string;
}

export interface ServerInfo {
  server_name: string;
  motd: string;
  ice_servers: IceServer[];
}

export type ActiveChannel = "global" | "session";
export type ConnectionStatus = "disconnected" | "connecting" | "connected";
