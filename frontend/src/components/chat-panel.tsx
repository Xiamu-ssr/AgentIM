"use client";

import { useRef, useEffect, useState } from "react";
import type { MessageResponse } from "@/api/types.generated";
import { ChatMessage } from "./chat-message";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";

interface ChatPanelProps {
  messages: MessageResponse[];
  currentAgentId: string;
  onSend: (content: string) => void;
  loading?: boolean;
  title?: string;
}

export function ChatPanel({
  messages,
  currentAgentId,
  onSend,
  loading,
  title,
}: ChatPanelProps) {
  const [input, setInput] = useState("");
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const trimmed = input.trim();
    if (!trimmed) return;
    onSend(trimmed);
    setInput("");
  }

  return (
    <div className="flex h-full flex-col">
      {title && (
        <div className="border-b border-border px-4 py-3">
          <h3 className="font-medium">{title}</h3>
        </div>
      )}

      <ScrollArea className="flex-1 p-4">
        {loading ? (
          <div className="flex h-full items-center justify-center text-muted-foreground">
            Loading messages...
          </div>
        ) : messages.length === 0 ? (
          <div className="flex h-full items-center justify-center text-muted-foreground">
            No messages yet
          </div>
        ) : (
          <div className="space-y-1">
            {messages.map((msg) => (
              <ChatMessage
                key={msg.id}
                message={msg}
                isOwn={msg.from_agent === currentAgentId}
              />
            ))}
            <div ref={bottomRef} />
          </div>
        )}
      </ScrollArea>

      <form
        onSubmit={handleSubmit}
        className="border-t border-border p-4"
      >
        <div className="flex gap-2">
          <Input
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="Type a message..."
            className="flex-1"
          />
          <Button type="submit" disabled={!input.trim()}>
            Send
          </Button>
        </div>
      </form>
    </div>
  );
}
