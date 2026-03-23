"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { createAgent } from "@/api/client";
import type { CreateAgentResponse } from "@/api/types.generated";

interface CreateAgentDialogProps {
  onCreated: (agent: CreateAgentResponse) => void;
}

export function CreateAgentDialog({ onCreated }: CreateAgentDialogProps) {
  const [open, setOpen] = useState(false);
  const [id, setId] = useState("");
  const [name, setName] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [createdToken, setCreatedToken] = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const agentId = id.trim();
    const displayName = name.trim() || agentId;

    if (!agentId) return;
    setSubmitting(true);
    setError(null);
    try {
      const result = await createAgent({
        id: agentId,
        name: displayName,
        bio: null,
        avatar_url: null,
      });
      setCreatedToken(result.token);
      onCreated(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create agent");
    } finally {
      setSubmitting(false);
    }
  }

  function handleClose() {
    setOpen(false);
    setId("");
    setName("");
    setError(null);
    setCreatedToken(null);
  }

  return (
    <Dialog open={open} onOpenChange={(v) => (v ? setOpen(true) : handleClose())}>
      <DialogTrigger render={<Button />}>
        Create New Agent
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create Agent</DialogTitle>
          <DialogDescription>
            Create a new AI agent. You will receive an API token after creation.
          </DialogDescription>
        </DialogHeader>

        {createdToken ? (
          <div className="space-y-3">
            <p className="text-sm text-muted-foreground">
              Agent created successfully. Copy the token below — it will not be shown again.
            </p>
            <div className="rounded-md bg-muted p-3">
              <code className="break-all text-sm">{createdToken}</code>
            </div>
            <DialogFooter>
              <Button onClick={handleClose}>Done</Button>
            </DialogFooter>
          </div>
        ) : (
          <form onSubmit={handleSubmit}>
            <div className="space-y-4 py-2">
              <div className="space-y-2">
                <Label htmlFor="agent-id">Agent ID</Label>
                <Input
                  id="agent-id"
                  value={id}
                  onChange={(e) => setId(e.target.value)}
                  placeholder="test-agent-01"
                  disabled={submitting}
                />
                <p className="text-xs text-muted-foreground">
                  Use lowercase letters, numbers, and hyphens only.
                </p>
              </div>
              <div className="space-y-2">
                <Label htmlFor="agent-name">Display Name</Label>
                <Input
                  id="agent-name"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="Optional, defaults to the agent ID"
                  disabled={submitting}
                />
              </div>
              {error && (
                <p className="text-sm text-destructive">{error}</p>
              )}
            </div>
            <DialogFooter>
              <Button type="button" variant="outline" onClick={handleClose}>
                Cancel
              </Button>
              <Button type="submit" disabled={submitting || !id.trim()}>
                {submitting ? "Creating..." : "Create"}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}
