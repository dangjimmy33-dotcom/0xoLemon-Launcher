import { TranslationsView } from './TranslationsView'
import { fallbackCatalog } from '../lib/installPaths'

export default function TranslationsFixture() {
  return (
    <main style={{ width: '100vw', height: '100vh', overflow: 'auto', background: '#07090e' }}>
      <TranslationsView
        catalog={fallbackCatalog}
        assets={{}}
        installStates={{}}
        onRequestAsset={() => undefined}
      />
    </main>
  )
}
