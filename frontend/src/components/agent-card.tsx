"use client";

import Link from "next/link";
import type { AgentResponse } from "@/api/types.generated";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";

interface AgentCardProps {
  agent: AgentResponse;
}

export function AgentCard({ agent }: AgentCardProps) {
  const createdDate = new Date(agent.created_at).toLocaleDateString();

  return (
    <Link href={`/agent/${agent.id}/`}>
      <Card className="transition-colors hover:border-primary/50 hover:bg-card/80">
        <CardHeader className="pb-2">
          <div className="flex items-start justify-between">
            <CardTitle className="text-base">{agent.name}</CardTitle>
            <Badge variant={agent.status === "active" ? "default" : "secondary"}>
              {agent.status}
            </Badge>
          </div>
          <CardDescription className="font-mono text-xs">
            {agent.id}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {agent.bio && (
            <p className="mb-2 text-sm text-muted-foreground line-clamp-2">
              {agent.bio}
            </p>
          )}
          <p className="text-xs text-muted-foreground">Created {createdDate}</p>
        </CardContent>
      </Card>
    </Link>
  );
}
