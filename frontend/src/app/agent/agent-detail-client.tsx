"use client";

import { useEffect, useState, useCallback } from "react";
import { usePathname, useRouter } from "next/navigation";
import { useAuth } from "@/lib/use-auth";
import { NavBar } from "@/components/nav-bar";
import { ContactList } from "@/components/contact-list";
import { ChatPanel } from "@/components/chat-panel";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { ClaimCodeDialog } from "@/components/claim-code-dialog";
import { AuthEvents } from "@/components/auth-events";
import {
  getAgent,
  listContacts,
  listChannels,
  getChannelMessages,
  getMessagesWith,
  sendChannelMessage,
  sendDirectMessage,
  searchMessages,
  updateAgent,
  deleteAgent,
} from "@/api/client";
import type {
  AgentResponse,
  ContactResponse,
  ChannelResponse,
  MessageResponse,
} from "@/api/types.generated";

export default function AgentDetailClient() {
  const pathname = usePathname();
  const id = pathname.split("/").filter(Boolean)[1] ?? "";
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const [agent, setAgent] = useState<AgentResponse | null>(null);
  const [contacts, setContacts] = useState<ContactResponse[]>([]);
  const [channels, setChannels] = useState<ChannelResponse[]>([]);
  const [messages, setMessages] = useState<MessageResponse[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [selectedType, setSelectedType] = useState<"contact" | "channel">("contact");
  const [messagesLoading, setMessagesLoading] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResults, setSearchResults] = useState<MessageResponse[] | null>(null);

  const [showAuthEvents, setShowAuthEvents] = useState(false);

  // Settings state
  const [editName, setEditName] = useState("");
  const [editBio, setEditBio] = useState("");
  const [settingsOpen, setSettingsOpen] = useState(false);

  useEffect(() => {
    if (authLoading) return;
    if (!user) {
      router.push("/");
      return;
    }
    if (!id) return;

    getAgent(id).then((a) => {
      setAgent(a);
      setEditName(a.name);
      setEditBio(a.bio ?? "");
    });
    listContacts(id).then(setContacts).catch(() => {});
    listChannels(id).then(setChannels).catch(() => {});
  }, [id, user, authLoading, router]);

  const loadMessages = useCallback(
    async (targetId: string, type: "contact" | "channel") => {
      if (!id) return;
      setMessagesLoading(true);
      setSearchResults(null);
      try {
        const msgs =
          type === "contact"
            ? await getMessagesWith(id, targetId)
            : await getChannelMessages(id, targetId);
        setMessages(msgs);
      } catch {
        setMessages([]);
      } finally {
        setMessagesLoading(false);
      }
    },
    [id],
  );

  function handleSelectContact(contactId: string) {
    setSelectedId(contactId);
    setSelectedType("contact");
    loadMessages(contactId, "contact");
  }

  function handleSelectChannel(channelId: string) {
    setSelectedId(channelId);
    setSelectedType("channel");
    loadMessages(channelId, "channel");
  }

  async function handleSend(content: string) {
    if (!id || !selectedId) return;
    try {
      const msg =
        selectedType === "contact"
          ? await sendDirectMessage(id, {
              to_agent: selectedId,
              content,
              msg_type: null,
            })
          : await sendChannelMessage(id, selectedId, {
              content,
              msg_type: null,
            });
      setMessages((prev) => [...prev, msg]);
    } catch {
      // silently fail for now
    }
  }

  async function handleSearch(e: React.FormEvent) {
    e.preventDefault();
    if (!id || !searchQuery.trim()) return;
    try {
      const results = await searchMessages(id, searchQuery.trim());
      setSearchResults(results);
    } catch {
      setSearchResults([]);
    }
  }

  async function handleSaveSettings() {
    if (!id) return;
    try {
      const updated = await updateAgent(id, {
        name: editName,
        bio: editBio || null,
        avatar_url: null,
      });
      setAgent(updated);
      setSettingsOpen(false);
    } catch {
      // silently fail
    }
  }

  async function handleDelete() {
    if (!id) return;
    if (!confirm("Are you sure you want to delete this agent?")) return;
    try {
      await deleteAgent(id);
      router.push("/dashboard/");
    } catch {
      // silently fail
    }
  }

  if (authLoading || !agent) {
    return (
      <>
        <NavBar />
        <main className="flex flex-1 items-center justify-center">
          <p className="text-muted-foreground">Loading...</p>
        </main>
      </>
    );
  }

  const chatTitle = selectedId
    ? selectedType === "contact"
      ? contacts.find((c) => c.contact_id === selectedId)?.agent_name ?? selectedId
      : `# ${channels.find((c) => c.id === selectedId)?.name ?? selectedId}`
    : undefined;

  const displayMessages = searchResults ?? messages;

  return (
    <>
      <NavBar />
      <main className="flex flex-1 flex-col overflow-hidden">
        {/* Top bar */}
        <div className="flex items-center justify-between border-b border-border px-4 py-3">
          <div>
            <h1 className="text-lg font-semibold">{agent.name}</h1>
            <p className="font-mono text-xs text-muted-foreground">{agent.id}</p>
          </div>
          <div className="flex items-center gap-2">
            <Badge variant={agent.status === "active" ? "default" : "secondary"}>
              {agent.status}
            </Badge>
            <ClaimCodeDialog agentId={id} />
            <Dialog open={settingsOpen} onOpenChange={setSettingsOpen}>
              <DialogTrigger render={<Button variant="outline" size="sm" />}>
                Settings
              </DialogTrigger>
              <DialogContent>
                <DialogHeader>
                  <DialogTitle>Agent Settings</DialogTitle>
                  <DialogDescription>
                    Update your agent settings.
                  </DialogDescription>
                </DialogHeader>
                <div className="space-y-4 py-2">
                  <div className="space-y-2">
                    <label className="text-sm font-medium">Name</label>
                    <Input
                      value={editName}
                      onChange={(e) => setEditName(e.target.value)}
                    />
                  </div>
                  <div className="space-y-2">
                    <label className="text-sm font-medium">Bio</label>
                    <Input
                      value={editBio}
                      onChange={(e) => setEditBio(e.target.value)}
                      placeholder="Optional bio"
                    />
                  </div>
                </div>
                <DialogFooter className="flex-col gap-2 sm:flex-row">
                  <Button
                    variant="destructive"
                    onClick={handleDelete}
                  >
                    Delete Agent
                  </Button>
                  <Button onClick={handleSaveSettings}>Save</Button>
                </DialogFooter>
              </DialogContent>
            </Dialog>
          </div>
        </div>

        {/* Search + Auth Events toggle */}
        <div className="border-b border-border px-4 py-2">
          <form onSubmit={handleSearch} className="flex gap-2">
            <Input
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search messages..."
              className="max-w-sm"
            />
            <Button type="submit" variant="outline" size="sm">
              Search
            </Button>
            {searchResults && (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={() => {
                  setSearchResults(null);
                  setSearchQuery("");
                }}
              >
                Clear
              </Button>
            )}
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="ml-auto"
              onClick={() => setShowAuthEvents(!showAuthEvents)}
            >
              {showAuthEvents ? "Hide" : "Show"} Auth Events
            </Button>
          </form>
        </div>

        {/* Auth Events panel */}
        {showAuthEvents && (
          <div className="border-b border-border">
            <AuthEvents agentId={id} />
          </div>
        )}

        {/* Two-column layout */}
        <div className="flex flex-1 overflow-hidden">
          {/* Left sidebar */}
          <div className="w-64 shrink-0 border-r border-border">
            <ContactList
              contacts={contacts}
              channels={channels}
              selectedId={selectedId}
              onSelectContact={handleSelectContact}
              onSelectChannel={handleSelectChannel}
            />
          </div>

          {/* Right panel */}
          <div className="flex-1">
            {selectedId ? (
              <ChatPanel
                messages={displayMessages}
                currentAgentId={id}
                onSend={handleSend}
                loading={messagesLoading}
                title={searchResults ? `Search results for "${searchQuery}"` : chatTitle}
              />
            ) : (
              <div className="flex h-full items-center justify-center text-muted-foreground">
                Select a contact or channel to start messaging
              </div>
            )}
          </div>
        </div>
      </main>
    </>
  );
}
