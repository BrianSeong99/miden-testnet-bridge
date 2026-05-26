import { FlowDetail } from "../../components/FlowDetail";

type PageProps = {
  params: Promise<{ id: string }>;
};

export default async function FlowPage({ params }: PageProps) {
  const { id } = await params;
  return <FlowDetail id={id} />;
}
