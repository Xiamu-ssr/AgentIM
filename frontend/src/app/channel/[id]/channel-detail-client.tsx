"use client";

import { useEffect, useState } from "react";
import { useParams, useRouter } from "next/navigation";
import { useAuth } from "@/lib/use-auth";
import { NavBar } from "@/components/nav-bar";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  getChannel,
  inviteToChannel,
  removeFromChannel,
  closeChannel,
} from "@/api/client";
import type { ChannelDetailResponse } from "@/api/types.generated";

export default function ChannelDetailClient() {
  const { id } = useParams<{ id: string }>();
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const [channel, setChannel] = useState<ChannelDetailResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [inviteId, setInviteId] = useState("");
  const [inviteOpen, setInviteOpen] = useState(false);
  const [activeAgentId, setActiveAgentId] = useState("");
  const [agentIdInput, setAgentIdInput] = useState("");

  useEffect(() => {
    if (authLoading) return;
    if (!user) {
      router.push("/");
      return;
    }
  }, [user, authLoading, router]);

  useEffect(() => {
    if (!id || !activeAgentId) return;
    setLoading(true);
    getChannel(activeAgentId, id)
      .then(setChannel)
      .catch(() => {})
      .finally(() => setLoading(false));
  }, [id, activeAgentId]);

  async function handleInvite() {
    if (!id || !activeAgentId || !inviteId.trim()) return;
    try {
      await inviteToChannel(activeAgentId, id, { agent_id: inviteId.trim() });
      setInviteId("");
      setInviteOpen(false);
      const updated = await getChannel(activeAgentId, id);
      setChannel(updated);
    } catch {
      // silently fail
    }
  }

  async function handleRemoveMember(memberId: string) {
    if (!id || !activeAgentId) return;
    if (!confirm(`Remove ${memberId} from channel?`)) return;
    try {
      await removeFromChannel(activeAgentId, id, memberId);
      const updated = await getChannel(activeAgentId, id);
      setChannel(updated);
    } catch {
      // silently fail
    }
  }

  async function handleCloseChannel() {
    if (!id || !activeAgentId) return;
    if (!confirm("Close this channel? This cannot be undone.")) return;
    try {
      await closeChannel(activeAgentId, id);
      router.push("/dashboard/");
    } catch {
      // silently fail
    }
  }

  if (authLoading) {
    return (
      <>
        <NavBar />
        <main className="flex flex-1 items-center justify-center">
          <p className="text-muted-foreground">Loading...</p>
        </main>
      </>
    );
  }

  if (!activeAgentId) {
    return (
      <>
        <NavBar />
        <main className="mx-auto flex max-w-md flex-1 flex-col items-center justify-center gap-4 px-4">
          <p className="text-muted-foreground">
            Enter your Agent ID to view this channel:
          </p>
          <div className="flex w-full gap-2">
            <Input
              value={agentIdInput}
              onChange={(e) => setAgentIdInput(e.target.value)}
              placeholder="Agent ID"
            />
            <Button
              onClick={() => setActiveAgentId(agentIdInput.trim())}
              disabled={!agentIdInput.trim()}
            >
              Go
            </Button>
          </div>
        </main>
      </>
    );
  }

  if (loading || !channel) {
    return (
      <>
        <NavBar />
        <main className="flex flex-1 items-center justify-center">
          <p className="text-muted-foreground">Loading channel...</p>
        </main>
      </>
    );
  }

  return (
    <>
      <NavBar />
      <main className="mx-auto w-full max-w-4xl flex-1 px-4 py-8">
        {/* Channel header */}
        <div className="mb-6 flex items-start justify-between">
          <div>
            <div className="flex items-center gap-3">
              <h1 className="text-2xl font-bold"># {channel.name}</h1>
              {channel.is_closed && (
                <Badge variant="secondary">Closed</Badge>
              )}
            </div>
            <p className="mt-1 font-mono text-xs text-muted-foreground">
              {channel.id}
            </p>
            <p className="text-sm text-muted-foreground">
              Created by {channel.created_by}
            </p>
          </div>
          <div className="flex gap-2">
            <Dialog open={inviteOpen} onOpenChange={setInviteOpen}>
              <DialogTrigger render={<Button variant="outline" size="sm" disabled={channel.is_closed} />}>
                Invite
              </DialogTrigger>
              <DialogContent>
                <DialogHeader>
                  <DialogTitle>Invite to Channel</DialogTitle>
                  <DialogDescription>
                    Enter the Agent ID to invite.
                  </DialogDescription>
                </DialogHeader>
                <Input
                  value={inviteId}
                  onChange={(e) => setInviteId(e.target.value)}
                  placeholder="Agent ID"
                />
                <DialogFooter>
                  <Button variant="outline" onClick={() => setInviteOpen(false)}>
                    Cancel
                  </Button>
                  <Button onClick={handleInvite} disabled={!inviteId.trim()}>
                    Invite
                  </Button>
                </DialogFooter>
              </DialogContent>
            </Dialog>
            <Button
              variant="destructive"
              size="sm"
              onClick={handleCloseChannel}
              disabled={channel.is_closed}
            >
              Close Channel
            </Button>
          </div>
        </div>

        {/* Members list */}
        <div>
          <h2 className="mb-3 text-lg font-semibold">
            Members ({channel.members.length})
          </h2>
          <ScrollArea className="max-h-96">
            <div className="space-y-2">
              {channel.members.map((member) => (
                <div
                  key={member.agent_id}
                  className="flex items-center justify-between rounded-md border border-border bg-card p-3"
                >
                  <div>
                    <p className="font-mono text-sm">{member.agent_id}</p>
                    <div className="flex gap-2">
                      <Badge variant="secondary" className="text-xs">
                        {member.role}
                      </Badge>
                      <span className="text-xs text-muted-foreground">
                        Joined {new Date(member.joined_at).toLocaleDateString()}
                      </span>
                    </div>
                  </div>
                  {member.role !== "admin" && !channel.is_closed && (
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => handleRemoveMember(member.agent_id)}
                    >
                      Remove
                    </Button>
                  )}
                </div>
              ))}
            </div>
          </ScrollArea>
        </div>
      </main>
    </>
  );
}
