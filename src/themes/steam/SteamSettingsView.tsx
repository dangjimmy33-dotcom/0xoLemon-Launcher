import { SettingsView, type SettingsViewProps } from '../../components/SettingsView'
import './SteamSettingsView.css'

export default function SteamSettingsView(props: SettingsViewProps) {
  return <SettingsView {...props} presentation="steam" />
}
