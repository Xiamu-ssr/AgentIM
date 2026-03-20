import type {
  AgentResponse,
  ChannelDetailResponse,
  ChannelResponse,
  ContactResponse,
  CreateAgentResponse,
  MeResponse,
  MessageResponse,
  ResetTokenResponse,
} from "./types.generated";

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

export function createAgent(name: string): Promise<CreateAgentResponse> {
  return request("/api/agents", {
    method: "POST",
    body: JSON.stringify({ name }),
  });
}

export function updateAgent(
  id: string,
  data: { name?: string; bio?: string },
): Promise<AgentResponse> {
  return request(`/api/agents/${id}`, {
    method: "PUT",
    body: JSON.stringify(data),
  });
}

export function deleteAgent(id: string): Promise<void> {
  return request(`/api/agents/${id}`, { method: "DELETE" });
}

export function resetAgentToken(id: string): Promise<ResetTokenResponse> {
  return request(`/api/agents/${id}/token/reset`, { method: "POST" });
}

// Contacts
export function listContacts(agentId: string): Promise<ContactResponse[]> {
  return request(`/api/contacts`, {
    headers: { "X-Agent-Id": agentId },
  });
}

// Messages
export function sendMessage(
  agentId: string,
  data: { to_agent?: string; channel_id?: string; content: string },
): Promise<MessageResponse> {
  return request("/api/messages", {
    method: "POST",
    headers: { "X-Agent-Id": agentId },
    body: JSON.stringify(data),
  });
}

export function getInbox(agentId: string): Promise<MessageResponse[]> {
  return request("/api/messages/inbox", {
    headers: { "X-Agent-Id": agentId },
  });
}

export function getMessagesWith(
  agentId: string,
  otherAgentId: string,
): Promise<MessageResponse[]> {
  return request(`/api/messages/with/${otherAgentId}`, {
    headers: { "X-Agent-Id": agentId },
  });
}

export function markMessageRead(
  agentId: string,
  messageId: string,
): Promise<void> {
  return request(`/api/messages/${messageId}/read`, {
    method: "POST",
    headers: { "X-Agent-Id": agentId },
  });
}

export function markAllRead(agentId: string): Promise<void> {
  return request("/api/messages/read-all", {
    method: "POST",
    headers: { "X-Agent-Id": agentId },
  });
}

export function searchMessages(
  agentId: string,
  query: string,
): Promise<MessageResponse[]> {
  return request(`/api/messages/search?q=${encodeURIComponent(query)}`, {
    headers: { "X-Agent-Id": agentId },
  });
}

// Channels
export function listChannels(agentId: string): Promise<ChannelResponse[]> {
  return request("/api/channels", {
    headers: { "X-Agent-Id": agentId },
  });
}

export function getChannel(
  agentId: string,
  channelId: string,
): Promise<ChannelDetailResponse> {
  return request(`/api/channels/${channelId}`, {
    headers: { "X-Agent-Id": agentId },
  });
}

export function createChannel(
  agentId: string,
  name: string,
): Promise<ChannelResponse> {
  return request("/api/channels", {
    method: "POST",
    headers: { "X-Agent-Id": agentId },
    body: JSON.stringify({ name }),
  });
}

export function inviteToChannel(
  agentId: string,
  channelId: string,
  inviteeId: string,
): Promise<void> {
  return request(`/api/channels/${channelId}/members`, {
    method: "POST",
    headers: { "X-Agent-Id": agentId },
    body: JSON.stringify({ agent_id: inviteeId }),
  });
}

export function removeFromChannel(
  agentId: string,
  channelId: string,
  memberId: string,
): Promise<void> {
  return request(`/api/channels/${channelId}/members/${memberId}`, {
    method: "DELETE",
    headers: { "X-Agent-Id": agentId },
  });
}

export function closeChannel(
  agentId: string,
  channelId: string,
): Promise<void> {
  return request(`/api/channels/${channelId}`, {
    method: "DELETE",
    headers: { "X-Agent-Id": agentId },
  });
}
