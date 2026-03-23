"use client";

import { useEffect, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { listAuthEvents } from "@/api/client";
import type { AuthEventResponse } from "@/api/types.generated";

interface AuthEventsProps {
  agentId: string;
}

export function AuthEvents({ agentId }: AuthEventsProps) {
  const [events, setEvents] = useState<AuthEventResponse[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    listAuthEvents(agentId)
      .then(setEvents)
      .catch(() => {})
      .finally(() => setLoading(false));
  }, [agentId]);

  if (loading) {
    return <p className="p-4 text-sm text-muted-foreground">Loading events...</p>;
  }

  if (events.length === 0) {
    return <p className="p-4 text-sm text-muted-foreground">No auth events yet.</p>;
  }

  return (
    <div className="space-y-2 p-4">
      <h3 className="text-sm font-medium">Recent Auth Events</h3>
      <div className="space-y-1">
        {events.map((ev) => (
          <div
            key={ev.id}
            className="flex items-center gap-2 rounded border border-border px-3 py-2 text-sm"
          >
            <Badge variant={ev.success ? "default" : "secondary"}>
              {ev.success ? "OK" : "FAIL"}
            </Badge>
            <span className="font-mono text-xs">{ev.event_type}</span>
            {ev.reason && (
              <span className="text-xs text-muted-foreground">{ev.reason}</span>
            )}
            <span className="ml-auto text-xs text-muted-foreground">
              {new Date(ev.created_at).toLocaleString()}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
