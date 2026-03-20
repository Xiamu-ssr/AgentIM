import ChannelDetailClient from "./channel-detail-client";

export async function generateStaticParams() {
  return [{ id: "placeholder" }];
}

export const dynamicParams = false;

export default function ChannelDetailPage() {
  return <ChannelDetailClient />;
}
