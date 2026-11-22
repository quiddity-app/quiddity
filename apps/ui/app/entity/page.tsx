import { ClientPage } from '@/components/client-page';
import { EntityPage } from '@/components/entity-page';

export default function EntityRoute() {
  return (
    <ClientPage>
      <EntityPage />
    </ClientPage>
  );
}
