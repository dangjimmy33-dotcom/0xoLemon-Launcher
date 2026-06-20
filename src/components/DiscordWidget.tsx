import { useEffect, useState } from 'react'
import { motion } from 'motion/react'

type DiscordWidgetProps = {
  serverId: string
  onOpenDiscord: () => void
  reducedMotion?: boolean
}

type DiscordData = {
  name: string
  iconUrl: string | null
  onlineCount: number
  memberCount: number
}

export function DiscordWidget({ serverId, onOpenDiscord, reducedMotion }: DiscordWidgetProps) {
  const [data, setData] = useState<DiscordData | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    let mounted = true

    async function fetchDiscordData() {
      try {
        // 1. Fetch widget.json to get the active invite code
        const widgetRes = await fetch(`https://discord.com/api/guilds/${serverId}/widget.json`)
        if (!widgetRes.ok) throw new Error('Widget fetch failed')
        const widgetData = await widgetRes.json()
        
        let inviteCode = null
        if (widgetData.instant_invite) {
          const urlParts = widgetData.instant_invite.split('/')
          inviteCode = urlParts[urlParts.length - 1]
        }

        // 2. Fetch the invite API to get total member count
        if (inviteCode) {
          const inviteRes = await fetch(`https://discord.com/api/v9/invites/${inviteCode}?with_counts=true`)
          if (inviteRes.ok) {
            const inviteData = await inviteRes.json()
            if (mounted) {
              setData({
                name: inviteData.guild.name,
                iconUrl: inviteData.guild.icon ? `https://cdn.discordapp.com/icons/${inviteData.guild.id}/${inviteData.guild.icon}.png` : null,
                onlineCount: inviteData.approximate_presence_count || widgetData.presence_count || 0,
                memberCount: inviteData.approximate_member_count || 0
              })
              setLoading(false)
              return
            }
          }
        }

        // Fallback if invite API fails but widget succeeds
        if (mounted) {
          setData({
            name: widgetData.name,
            iconUrl: null, // Widget API doesn't provide standard guild icon hash easily without invite API
            onlineCount: widgetData.presence_count || 0,
            memberCount: 0
          })
          setLoading(false)
        }

      } catch (err) {
        console.error('Failed to fetch Discord data', err)
        if (mounted) setLoading(false)
      }
    }

    fetchDiscordData()
    return () => { mounted = false }
  }, [serverId])

  // If loading or error, we can either render a skeleton or the fallback default card
  if (loading || !data) {
    return (
      <motion.section className="home-side-card community-card" whileHover={reducedMotion ? undefined : { y: -3 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
          <div style={{ width: '48px', height: '48px', borderRadius: '12px', background: 'rgba(255,255,255,0.1)' }} />
          <div>
            <h2>Join the community</h2>
            <p>Updates, help and discussion.</p>
          </div>
        </div>
        <button type="button" onClick={onOpenDiscord}>Open Discord</button>
      </motion.section>
    )
  }

  return (
    <motion.section className="home-side-card community-card discord-widget" whileHover={reducedMotion ? undefined : { y: -3 }}>
      <div className="discord-widget-header">
        {data.iconUrl ? (
          <img src={data.iconUrl} alt={data.name} className="discord-icon" />
        ) : (
          <div className="discord-icon-placeholder" />
        )}
        <div className="discord-info">
          <h2>{data.name}</h2>
          <div className="discord-stats">
            <span className="discord-stat">
              <span className="status-dot online"></span>
              {data.onlineCount.toLocaleString()} Online
            </span>
            {data.memberCount > 0 && (
              <span className="discord-stat">
                <span className="status-dot offline"></span>
                {data.memberCount.toLocaleString()} Members
              </span>
            )}
          </div>
        </div>
      </div>
      <button type="button" onClick={onOpenDiscord}>Join Server</button>
    </motion.section>
  )
}
