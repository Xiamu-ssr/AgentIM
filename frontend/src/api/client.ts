import type {
  AuthEventResponse,
  ChatHistoryParams,
  AgentResponse,
  ClaimCodeResponse,
  CreateAgentRequest,
  CreateChannelRequest,
  ChannelDetailResponse,
  ChannelResponse,
  ContactResponse,
  CreateAgentResponse,
  InviteMemberRequest,
  MeResponse,
  MessageResponse,
  SearchParams,
  SendChannelMessageRequest,
  SendMessageRequest,
  UpdateAgentRequest,
} from "./types.generated";
import { HEADER_AGENT_ID } from "./constants.generated";

const BASE_URL = process.env.NEXT_PUBLIC_API_BASE_URL ?? "";

async function request<T>(path: string, options?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE_URL}${path}`, {
    credentials: "include",
    headers: {
      "Content-Type": "application/json",
      ...options?.headers,
    },
    ...options,
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`API ${res.status}: ${text || res.statusText}`);
  }
  if (res.status === 204) return undefined as T;
  return res.json();
}

// Auth
export function getMe(): Promise<MeResponse> {
  return request("/api/auth/me");
}

export function getGitHubLoginUrl(): string {
  return `${BASE_URL}/api/auth/github`;
}

// Agents
export function listAgents(): Promise<AgentResponse[]> {
  return request("/api/agents");
}

export function getAgent(id: string): Promise<AgentResponse> {
  return request(`/api/agents/${id}`);
}

export function createAgent(data: CreateAgentRequest): Promise<CreateAgentResponse> {
  return request("/api/agents", {
    method: "POST",
    body: JSON.stringify(data),
  });
}

export function updateAgent(id: string, data: UpdateAgentRequest): Promise<AgentResponse> {
  return request(`/api/agents/${id}`, {
    method: "PUT",
    body: JSON.stringify(data),
  });
}

export function deleteAgent(id: string): Promise<void> {
  return request(`/api/agents/${id}`, { method: "DELETE" });
}

// Credentials & Auth Events
export function generateClaimCode(agentId: string): Promise<ClaimCodeResponse> {
  return request(`/api/agents/${agentId}/claim`, { method: "POST" });
}

export function listAuthEvents(agentId: string): Promise<AuthEventResponse[]> {
  return request(`/api/agents/${agentId}/auth-events`);
}

// Contacts
export function listContacts(agentId: string): Promise<ContactResponse[]> {
  return request(`/api/contacts`, {
    headers: { [HEADER_AGENT_ID]: agentId },
  });
}

// Messages
export function sendDirectMessage(
  agentId: string,
  data: SendMessageRequest,
): Promise<MessageResponse> {
  return request("/api/messages", {
    method: "POST",
    headers: { [HEADER_AGENT_ID]: agentId },
    body: JSON.stringify(data),
  });
}

export function getInbox(agentId: string): Promise<MessageResponse[]> {
  return request("/api/messages/inbox", {
    headers: { [HEADER_AGENT_ID]: agentId },
  });
}

export function getMessagesWith(
  agentId: string,
  otherAgentId: string,
  params?: ChatHistoryParams,
): Promise<MessageResponse[]> {
  const query = new URLSearchParams();
  if (params?.limit !== undefined) query.set("limit", String(params.limit));
  if (params?.before) query.set("before", params.before);
  const suffix = query.size > 0 ? `?${query.toString()}` : "";

  return request(`/api/messages/with/${otherAgentId}${suffix}`, {
    headers: { [HEADER_AGENT_ID]: agentId },
  });
}

export function markMessageRead(
  agentId: string,
  messageId: string,
): Promise<void> {
  return request(`/api/messages/${messageId}/read`, {
    method: "POST",
    headers: { [HEADER_AGENT_ID]: agentId },
  });
}

export function markAllRead(agentId: string): Promise<void> {
  return request("/api/messages/read-all", {
    method: "POST",
    headers: { [HEADER_AGENT_ID]: agentId },
  });
}

export function searchMessages(
  agentId: string,
  query: SearchParams["q"],
): Promise<MessageResponse[]> {
  return request(`/api/messages/search?q=${encodeURIComponent(query)}`, {
    headers: { [HEADER_AGENT_ID]: agentId },
  });
}

// Channels
export function listChannels(agentId: string): Promise<ChannelResponse[]> {
  return request("/api/channels", {
    headers: { [HEADER_AGENT_ID]: agentId },
  });
}

export function getChannel(
  agentId: string,
  channelId: string,
): Promise<ChannelDetailResponse> {
  return request(`/api/channels/${channelId}`, {
    headers: { [HEADER_AGENT_ID]: agentId },
  });
}

export function createChannel(
  agentId: string,
  data: CreateChannelRequest,
): Promise<ChannelResponse> {
  return request("/api/channels", {
    method: "POST",
    headers: { [HEADER_AGENT_ID]: agentId },
    body: JSON.stringify(data),
  });
}

export function inviteToChannel(
  agentId: string,
  channelId: string,
  data: InviteMemberRequest,
): Promise<void> {
  return request(`/api/channels/${channelId}/members`, {
    method: "POST",
    headers: { [HEADER_AGENT_ID]: agentId },
    body: JSON.stringify(data),
  });
}

export function removeFromChannel(
  agentId: string,
  channelId: string,
  memberId: string,
): Promise<void> {
  return request(`/api/channels/${channelId}/members/${memberId}`, {
    method: "DELETE",
    headers: { [HEADER_AGENT_ID]: agentId },
  });
}

export function closeChannel(
  agentId: string,
  channelId: string,
): Promise<void> {
  return request(`/api/channels/${channelId}/close`, {
    method: "POST",
    headers: { [HEADER_AGENT_ID]: agentId },
  });
}

export function sendChannelMessage(
  agentId: string,
  channelId: string,
  data: SendChannelMessageRequest,
): Promise<MessageResponse> {
  return request(`/api/channels/${channelId}/messages`, {
    method: "POST",
    headers: { [HEADER_AGENT_ID]: agentId },
    body: JSON.stringify(data),
  });
}

export function getChannelMessages(
  agentId: string,
  channelId: string,
  params?: ChatHistoryParams,
): Promise<MessageResponse[]> {
  const query = new URLSearchParams();
  if (params?.limit !== undefined) query.set("limit", String(params.limit));
  if (params?.before) query.set("before", params.before);
  const suffix = query.size > 0 ? `?${query.toString()}` : "";

  return request(`/api/channels/${channelId}/messages${suffix}`, {
    headers: { [HEADER_AGENT_ID]: agentId },
  });
}
