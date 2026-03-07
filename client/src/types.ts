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
  /** True for join/leave notifications, rendered differently, never sent over the wire */
  system?: boolean;
}

export interface SessionInfo {
  id: string;
  name: string;
  memberCount: number;
  isPublic: boolean;
  maxMembers: number;
  gameStarted: boolean;
}

export interface SessionPayload {
  sessionId: string;
  sessionName: string;
  isPublic: boolean;
  maxMembers: number;
  existingPeers: string[];
  host: string;
  gameStarted: boolean;
}

export type PlayerRole = "Player" | "Spectator" | "Inactive";

export interface LaunchMode {
  id: string;
  /** Display name and platform identifier, e.g. "SNES9x" */
  name: string;
  /** Path to the executable */
  exePath: string;
  /** Path to folder containing games, omit for standalone executables */
  gamesFolder?: string;
  /** SHA-256 of the executable, computed once when the mode is added */
  exeHash?: string;
}

export interface UserConfig {
  launchModes: LaunchMode[];
  debugLog?: boolean;
}

export interface MemberAssignment {
  role: PlayerRole;
  playerId: number;
}

export interface LaunchConfig {
  game: string;
  platform: string;
  members: Record<string, MemberAssignment>;
  /** SHA-256 of the host's emulator executable */
  exeHash?: string;
  /** SHA-256 of the host's game file */
  gameHash?: string;
}

export interface IceServer {
  url: string;
  username: string;
  credential: string;
  name: string;
}

export interface ServerInfo {
  serverName: string;
  motd: string;
  iceServers: IceServer[];
}

export type ActiveChannel = "global" | "session";
export type ConnectionStatus = "disconnected" | "connecting" | "connected";
