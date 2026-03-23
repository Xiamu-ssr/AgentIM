import AgentDetailClient from "./agent-detail-client";

export async function generateStaticParams() {
  return [{ id: "placeholder" }];
}

export const dynamicParams = true;

export default function AgentDetailPage() {
  return <AgentDetailClient />;
}
