export interface SavedServer {
  id: string;
  displayName: string;
  host: string;
  port: number;
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
  game_started: boolean;
}

export interface SessionPayload {
  session_id: string;
  session_name: string;
  is_public: boolean;
  max_members: number;
  existing_peers: string[];
  host: string;
  game_started: boolean;
}

export type PlayerRole = "Player" | "Spectator" | "Inactive";

export interface LaunchMode {
  id: string;
  /** Display name and platform identifier, e.g. "SNES9x" */
  name: string;
  /** Path to the executable */
  exePath: string;
  /** Path to folder containing games — omit for standalone executables */
  gamesFolder?: string;
}

export interface UserConfig {
  launchModes: LaunchMode[];
}

export interface MemberAssignment {
  role: PlayerRole;
  player_id: number;
}

export interface LaunchConfig {
  game: string;
  platform: string;
  members: Record<string, MemberAssignment>;
  /** SHA-256 of the host's emulator executable */
  exe_hash?: string;
  /** SHA-256 of the host's game file */
  game_hash?: string;
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
