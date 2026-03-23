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
import { generateClaimCode } from "@/api/client";

interface ClaimCodeDialogProps {
  agentId: string;
}

export function ClaimCodeDialog({ agentId }: ClaimCodeDialogProps) {
  const [open, setOpen] = useState(false);
  const [claimCode, setClaimCode] = useState<string | null>(null);
  const [expiresAt, setExpiresAt] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  async function handleGenerate() {
    setLoading(true);
    setError(null);
    try {
      const resp = await generateClaimCode(agentId);
      setClaimCode(resp.claim_code);
      setExpiresAt(resp.expires_at);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to generate claim code");
    } finally {
      setLoading(false);
    }
  }

  async function handleCopy() {
    if (!claimCode) return;
    try {
      await navigator.clipboard.writeText(claimCode);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // fallback: select text
    }
  }

  function handleClose() {
    setOpen(false);
    setClaimCode(null);
    setExpiresAt(null);
    setError(null);
    setCopied(false);
  }

  const expiresDisplay = expiresAt
    ? new Date(expiresAt).toLocaleTimeString()
    : null;

  return (
    <Dialog open={open} onOpenChange={(v) => (v ? setOpen(true) : handleClose())}>
      <DialogTrigger render={<Button variant="outline" size="sm" />}>
        Generate Claim Code
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Claim Code</DialogTitle>
          <DialogDescription>
            Generate a one-time code to bind an Ed25519 keypair to this agent
            via the CLI.
          </DialogDescription>
        </DialogHeader>

        {claimCode ? (
          <div className="space-y-3">
            <div className="rounded-md border border-border bg-muted p-3">
              <code className="select-all break-all text-sm">{claimCode}</code>
            </div>
            <p className="text-xs text-muted-foreground">
              Expires at {expiresDisplay}. Use this code with:
            </p>
            <pre className="rounded-md bg-muted p-2 text-xs">
              agentim init --agent-id {agentId} --claim &lt;code&gt;
            </pre>
            <DialogFooter>
              <Button variant="outline" onClick={handleCopy}>
                {copied ? "Copied!" : "Copy Code"}
              </Button>
              <Button onClick={handleClose}>Done</Button>
            </DialogFooter>
          </div>
        ) : (
          <div className="space-y-3">
            {error && <p className="text-sm text-destructive">{error}</p>}
            <p className="text-sm text-muted-foreground">
              This will generate a new claim code. Any existing active claim
              code for this agent will be revoked.
            </p>
            <DialogFooter>
              <Button variant="outline" onClick={handleClose}>
                Cancel
              </Button>
              <Button onClick={handleGenerate} disabled={loading}>
                {loading ? "Generating..." : "Generate"}
              </Button>
            </DialogFooter>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
