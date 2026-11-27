import { About } from '@/components/about';

// Needs no graph, so unlike the other pages it is drawn at export time and
// is there at once, which matters for notices that must always be reachable.
export default function AboutPage() {
  return <About />;
}
