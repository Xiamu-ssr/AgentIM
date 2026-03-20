"use client";

import type { ContactResponse, ChannelResponse } from "@/api/types.generated";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";

interface ContactListProps {
  contacts: ContactResponse[];
  channels: ChannelResponse[];
  selectedId: string | null;
  onSelectContact: (contactId: string) => void;
  onSelectChannel: (channelId: string) => void;
}

export function ContactList({
  contacts,
  channels,
  selectedId,
  onSelectContact,
  onSelectChannel,
}: ContactListProps) {
  return (
    <ScrollArea className="h-full">
      <div className="p-3">
        <h4 className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
          Contacts
        </h4>
        {contacts.length === 0 ? (
          <p className="px-2 text-sm text-muted-foreground">No contacts</p>
        ) : (
          <ul className="space-y-0.5">
            {contacts.map((c) => (
              <li key={c.contact_id}>
                <button
                  onClick={() => onSelectContact(c.contact_id)}
                  className={`w-full rounded-md px-2 py-1.5 text-left text-sm transition-colors hover:bg-muted ${
                    selectedId === c.contact_id
                      ? "bg-muted text-foreground"
                      : "text-muted-foreground"
                  }`}
                >
                  <span className="block truncate font-medium">
                    {c.alias ?? c.agent_name}
                  </span>
                  <span className="block truncate font-mono text-[10px] text-muted-foreground">
                    {c.contact_id}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}

        <Separator className="my-3" />

        <h4 className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
          Channels
        </h4>
        {channels.length === 0 ? (
          <p className="px-2 text-sm text-muted-foreground">No channels</p>
        ) : (
          <ul className="space-y-0.5">
            {channels.map((ch) => (
              <li key={ch.id}>
                <button
                  onClick={() => onSelectChannel(ch.id)}
                  className={`w-full rounded-md px-2 py-1.5 text-left text-sm transition-colors hover:bg-muted ${
                    selectedId === ch.id
                      ? "bg-muted text-foreground"
                      : "text-muted-foreground"
                  }`}
                >
                  <span className="block truncate font-medium">
                    # {ch.name}
                  </span>
                  {ch.is_closed && (
                    <span className="text-[10px] text-destructive">closed</span>
                  )}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </ScrollArea>
  );
}
