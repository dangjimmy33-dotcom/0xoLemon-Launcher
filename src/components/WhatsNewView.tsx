import { useCallback, useEffect, useMemo, useRef, useState, type ComponentType, type PointerEvent as ReactPointerEvent, type ReactNode, type RefObject } from 'react'
import {
  ArrowRight,
  Calendar,
  ChevronDown,
  ChevronUp,
  DownloadCloud,
  Gamepad2,
  Layers3,
  MonitorPlay,
  Palette,
  Radio,
  ShieldCheck,
  Sparkles,
  Tag,
  UsersRound,
  Zap,
} from 'lucide-react'
import {
  motion,
  useInView,
  useMotionValue,
  useReducedMotion,
  useScroll,
  useSpring,
  useTransform,
} from 'motion/react'
import { useLocale } from '../context/locale'
import CHANGELOG from '../changelog.json'
import socialShowcase1280Avif from '../assets/showcase/social-1280.avif'
import socialShowcase1280Webp from '../assets/showcase/social-1280.webp'
import socialShowcase1920Avif from '../assets/showcase/social-1920.avif'
import socialShowcase1920Webp from '../assets/showcase/social-1920.webp'
import bigPictureShowcase1280Avif from '../assets/showcase/big-picture-1280.avif'
import bigPictureShowcase1280Webp from '../assets/showcase/big-picture-1280.webp'
import bigPictureShowcase1920Avif from '../assets/showcase/big-picture-1920.avif'
import bigPictureShowcase1920Webp from '../assets/showcase/big-picture-1920.webp'
import personalizationShowcase1280Avif from '../assets/showcase/personalization-1280.avif'
import personalizationShowcase1280Webp from '../assets/showcase/personalization-1280.webp'
import personalizationShowcase1920Avif from '../assets/showcase/personalization-1920.avif'
import personalizationShowcase1920Webp from '../assets/showcase/personalization-1920.webp'
import smartDeliveryShowcase1280Avif from '../assets/showcase/smart-delivery-1280.avif'
import smartDeliveryShowcase1280Webp from '../assets/showcase/smart-delivery-1280.webp'
import smartDeliveryShowcase1920Avif from '../assets/showcase/smart-delivery-1920.avif'
import smartDeliveryShowcase1920Webp from '../assets/showcase/smart-delivery-1920.webp'
import './WhatsNewView.css'

type ShowcaseVisual = 'social' | 'big-picture' | 'themes' | 'downloads'
type FeatureCopy = { eyebrow: string; title: string; description: string; bullets: string[] }

type ProductFeature = {
  id: string
  icon: ComponentType<{ size?: number; className?: string }>
  visual: ShowcaseVisual
  en: FeatureCopy
  vi: FeatureCopy
}

type ProductCatalogItem = {
  id: string
  index: string
  name: string
  category: string
  status: string
  summary: { en: string; vi: string }
}

type ShowcaseMedia = {
  avif1280: string
  avif1920: string
  webp1280: string
  webp1920: string
}

const SHOWCASE_MEDIA: Record<ShowcaseVisual, ShowcaseMedia> = {
  social: {
    avif1280: socialShowcase1280Avif,
    avif1920: socialShowcase1920Avif,
    webp1280: socialShowcase1280Webp,
    webp1920: socialShowcase1920Webp,
  },
  'big-picture': {
    avif1280: bigPictureShowcase1280Avif,
    avif1920: bigPictureShowcase1920Avif,
    webp1280: bigPictureShowcase1280Webp,
    webp1920: bigPictureShowcase1920Webp,
  },
  themes: {
    avif1280: personalizationShowcase1280Avif,
    avif1920: personalizationShowcase1920Avif,
    webp1280: personalizationShowcase1280Webp,
    webp1920: personalizationShowcase1920Webp,
  },
  downloads: {
    avif1280: smartDeliveryShowcase1280Avif,
    avif1920: smartDeliveryShowcase1920Avif,
    webp1280: smartDeliveryShowcase1280Webp,
    webp1920: smartDeliveryShowcase1920Webp,
  },
}

export const PRODUCT_CATALOG: ProductCatalogItem[] = [
  {
    id: 'launcher',
    index: '01',
    name: '0xoLemon Launcher',
    category: 'Desktop gaming platform',
    status: 'Available now',
    summary: {
      en: 'A desktop home for games, downloads, social, personalization and big-screen play — designed as one coherent product.',
      vi: 'Một không gian desktop cho game, download, social, cá nhân hóa và big-screen play — được thiết kế như một sản phẩm thống nhất.',
    },
  },
]

export const PRODUCT_FEATURES: ProductFeature[] = [
  {
    id: 'social-layer',
    icon: UsersRound,
    visual: 'social',
    en: {
      eyebrow: 'SOCIAL, EVERYWHERE',
      title: 'Friends remain present while you use the launcher.',
      description: 'A Discord-inspired member layer lives beside Store, Library, Downloads and Settings instead of forcing you into a separate social screen.',
      bullets: ['Global member drawer', 'Profile popouts and full profiles', 'Friend requests and leaderboard-ready layout'],
    },
    vi: {
      eyebrow: 'SOCIAL Ở MỌI NƠI',
      title: 'Bạn bè luôn hiện diện khi bạn dùng launcher.',
      description: 'Lớp member lấy cảm hứng từ Discord nằm cạnh Store, Library, Downloads và Settings thay vì ép bạn chuyển sang một màn social riêng.',
      bullets: ['Member drawer toàn launcher', 'Profile popout và full profile', 'Sẵn bố cục cho kết bạn và leaderboard'],
    },
  },
  {
    id: 'big-picture',
    icon: MonitorPlay,
    visual: 'big-picture',
    en: {
      eyebrow: 'BIG PICTURE',
      title: 'A launcher that can leave the desktop behind.',
      description: 'A controller-first presentation for choosing a game and jumping into play from the couch without dragging desktop chrome along for the ride.',
      bullets: ['Fullscreen-first presentation', 'Controller-friendly focus and actions', 'Cinematic entry and exit motion'],
    },
    vi: {
      eyebrow: 'BIG PICTURE',
      title: 'Một launcher có thể rời khỏi cảm giác desktop.',
      description: 'Bề mặt ưu tiên controller để chọn game và vào chơi từ xa mà không kéo theo đống desktop chrome.',
      bullets: ['Trình bày ưu tiên fullscreen', 'Điều hướng thân thiện tay cầm', 'Animation vào/thoát cinematic'],
    },
  },
  {
    id: 'color-studio',
    icon: Palette,
    visual: 'themes',
    en: {
      eyebrow: 'PERSONALIZATION',
      title: 'One accent can shape the whole product.',
      description: 'Hue, saturation, intensity, contrast and dynamic motion flow through semantic surfaces instead of recoloring a handful of buttons.',
      bullets: ['Global semantic palette', 'Dynamic theme animation', 'Readable surfaces at every intensity'],
    },
    vi: {
      eyebrow: 'CÁ NHÂN HÓA',
      title: 'Một accent có thể định hình toàn bộ sản phẩm.',
      description: 'Hue, saturation, intensity, contrast và dynamic motion truyền qua semantic surface thay vì chỉ đổi màu vài nút.',
      bullets: ['Palette semantic toàn launcher', 'Dynamic theme animation', 'Giữ khả năng đọc ở mọi cường độ'],
    },
  },
  {
    id: 'downloads',
    icon: DownloadCloud,
    visual: 'downloads',
    en: {
      eyebrow: 'SMART DELIVERY',
      title: 'Progress that explains what the launcher is doing.',
      description: 'Download, assemble and recovery are shown as explicit states so the user sees a process rather than a spinner.',
      bullets: ['Clear download and assemble phases', 'Version-aware recovery', 'Readable progress at a glance'],
    },
    vi: {
      eyebrow: 'SMART DELIVERY',
      title: 'Tiến độ phải giải thích launcher đang làm gì.',
      description: 'Download, assemble và recovery được tách thành trạng thái rõ ràng để user nhìn thấy cả quá trình thay vì chỉ thấy spinner.',
      bullets: ['Download và assemble tách bạch', 'Recovery theo phiên bản', 'Tiến độ dễ đọc trong một ánh nhìn'],
    },
  },
]

const PRODUCT_COPY = {
  'en-US': {
    heroPrefix: 'Built to',
    heroWords: ['launch.', 'play.', 'connect.', 'feel yours.'],
    heroSubtitle: '0xoLemon is growing into a product family. The Launcher is product one — a desktop gaming experience built around speed, ownership and continuity.',
    versionLabel: 'Version',
    explore: 'Explore products',
    releases: 'Release notes',
    productKicker: 'PRODUCT FAMILY',
    productTitle: 'One brand. A growing line of products.',
    productSubtitle: 'This is the shelf for 0xoLemon products. Launcher is first; future products can join the same family without turning this page into a changelog wall.',
    exploreLauncher: 'Explore Launcher',
    highlightsKicker: 'LAUNCHER HIGHLIGHTS',
    highlightsTitle: 'See the product, then explore what makes it different.',
    highlightsSubtitle: 'Four scroll-driven chapters reveal the launcher continuously. Every transition follows your movement and reverses cleanly when you scroll back.',
    releaseKicker: 'RELEASE NOTES',
    releaseTitle: 'The detailed history stays available.',
    releaseSubtitle: 'The showcase explains the product. The changelog records exactly what shipped.',
    showAll: 'Show all releases',
    showLess: 'Show less',
    latest: 'LATEST',
    status: 'DESKTOP EXPERIENCE',
  },
  'vi-VN': {
    heroPrefix: 'Được tạo để',
    heroWords: ['khởi chạy.', 'chơi.', 'kết nối.', 'mang dấu ấn của bạn.'],
    heroSubtitle: '0xoLemon đang phát triển thành một hệ sản phẩm. Launcher là sản phẩm đầu tiên — trải nghiệm gaming desktop xoay quanh tốc độ, quyền kiểm soát và tính liên tục.',
    versionLabel: 'Phiên bản',
    explore: 'Khám phá sản phẩm',
    releases: 'Release notes',
    productKicker: 'HỆ SẢN PHẨM',
    productTitle: 'Một thương hiệu. Một dòng sản phẩm ngày càng mở rộng.',
    productSubtitle: 'Đây là khu trưng bày sản phẩm 0xoLemon. Launcher là sản phẩm đầu tiên; sau này có thể thêm sản phẩm mới mà không biến trang này thành một bức tường changelog.',
    exploreLauncher: 'Khám phá Launcher',
    highlightsKicker: 'ĐIỂM NỔI BẬT CỦA LAUNCHER',
    highlightsTitle: 'Nhìn sản phẩm trước, rồi khám phá điều làm nó khác biệt.',
    highlightsSubtitle: 'Bốn chapter cinematic chuyển động liên tục theo thao tác cuộn. Mọi chuyển cảnh đều đảo ngược chính xác khi bạn cuộn lên.',
    releaseKicker: 'RELEASE NOTES',
    releaseTitle: 'Lịch sử thay đổi chi tiết vẫn được giữ lại.',
    releaseSubtitle: 'Showcase giải thích sản phẩm; changelog ghi chính xác những gì đã được phát hành.',
    showAll: 'Hiện tất cả phiên bản',
    showLess: 'Thu gọn',
    latest: 'MỚI NHẤT',
    status: 'TRẢI NGHIỆM DESKTOP',
  },
} as const

const CONTOUR_PATHS = Array.from({ length: 34 }, (_, row) => {
  const baseY = -30 + row * 32
  const points: string[] = []
  for (let x = -140; x <= 1360; x += 42) {
    const y = baseY + Math.sin((x + row * 48) / 94) * 24 + Math.sin((x - row * 19) / 44) * 8
    points.push(`${x},${y.toFixed(1)}`)
  }
  return points.join(' ')
})

function useTypewriter(words: readonly string[], reducedMotion: boolean) {
  const [wordIndex, setWordIndex] = useState(0)
  const [text, setText] = useState(reducedMotion ? (words[0] ?? '') : '')
  const [deleting, setDeleting] = useState(false)

  useEffect(() => {
    const word = words[wordIndex] ?? ''
    if (reducedMotion) {
      setText(word)
      setDeleting(false)
      return
    }

    let delay = deleting ? 42 : 70
    if (!deleting && text === word) delay = 1250
    if (deleting && text === '') delay = 250

    const timeout = window.setTimeout(() => {
      if (!deleting && text === word) {
        setDeleting(true)
        return
      }
      if (deleting && text === '') {
        setDeleting(false)
        setWordIndex((current) => (current + 1) % Math.max(1, words.length))
        return
      }
      setText(deleting ? word.slice(0, Math.max(0, text.length - 1)) : word.slice(0, text.length + 1))
    }, delay)

    return () => window.clearTimeout(timeout)
  }, [deleting, reducedMotion, text, wordIndex, words])

  return text
}

function ContourField() {
  return (
    <svg viewBox="0 0 1200 900" preserveAspectRatio="xMidYMid slice">
      <g className="wn-contour-lines primary">
        {CONTOUR_PATHS.map((points, index) => <polyline key={`p-${index}`} points={points} />)}
      </g>
      <g className="wn-contour-lines secondary" transform="rotate(8 600 450) translate(26 -10)">
        {CONTOUR_PATHS.filter((_, index) => index % 2 === 0).map((points, index) => <polyline key={`s-${index}`} points={points} />)}
      </g>
    </svg>
  )
}

function ProductShowcaseVisual({ type, reducedMotion = false }: { type: ShowcaseVisual; reducedMotion?: boolean }) {
  if (type === 'social') {
    return (
      <div className="wn-visual wn-social-visual" aria-hidden="true">
        <div className="wn-social-main">
          <div className="wn-social-title"><UsersRound size={16} /><span>Friends</span><i>6 online</i></div>
          <div className="wn-social-search" />
          {['AK', 'MO', 'KA', 'LE'].map((name, index) => (
            <motion.div key={name} className="wn-social-person" animate={reducedMotion ? undefined : { x: [0, index % 2 ? 3 : -2, 0] }} transition={reducedMotion ? undefined : { duration: 3.4 + index * .35, repeat: Infinity }}>
              <b>{name}</b><span><strong>{['Akira', 'MoonByte', 'Kaito', 'LemonTea'][index]}</strong><small>{index < 2 ? 'Playing now' : index === 2 ? 'In Library' : 'Browsing Store'}</small></span><i />
            </motion.div>
          ))}
        </div>
        <div className="wn-social-rail">{['AK', 'MO', 'KA', 'LE'].map((name) => <i key={name}>{name}</i>)}</div>
      </div>
    )
  }

  if (type === 'big-picture') {
    return (
      <div className="wn-visual wn-big-picture-visual" aria-hidden="true">
        <div className="wn-bp-sky" />
        <div className="wn-bp-copy"><small>READY TO PLAY</small><strong>Among Us</strong><span>Controller-ready experience</span><button tabIndex={-1}><Gamepad2 size={14} /> Play</button></div>
        <div className="wn-bp-posters">
          {[0, 1, 2, 3].map((index) => <motion.i key={index} animate={reducedMotion ? undefined : { y: [0, index === 1 ? -7 : 0, 0] }} transition={reducedMotion ? undefined : { duration: 4, repeat: Infinity, delay: index * .14 }} />)}
        </div>
      </div>
    )
  }

  if (type === 'themes') {
    return (
      <div className="wn-visual wn-theme-visual" aria-hidden="true">
        <div className="wn-theme-card">
          <span className="wn-theme-chip"><Palette size={15} /> Color Studio</span>
          <motion.div className="wn-theme-wheel" animate={reducedMotion ? undefined : { rotate: 360 }} transition={reducedMotion ? undefined : { duration: 24, repeat: Infinity, ease: 'linear' }} />
          <div className="wn-theme-sliders"><span><i style={{ width: '78%' }} /></span><span><i style={{ width: '62%' }} /></span><span><i style={{ width: '88%' }} /></span></div>
          <div className="wn-theme-pills">{[0, 1, 2, 3, 4].map((item) => <i key={item} />)}</div>
        </div>
      </div>
    )
  }

  return (
    <div className="wn-visual wn-download-visual" aria-hidden="true">
      <div className="wn-download-head"><DownloadCloud size={17} /><span><strong>Installing</strong><small>007 First Light</small></span><b>72%</b></div>
      <div className="wn-download-progress"><motion.i animate={reducedMotion ? undefined : { width: ['38%', '72%', '88%'] }} transition={reducedMotion ? undefined : { duration: 4.5, repeat: Infinity, repeatType: 'reverse' }} /></div>
      <div className="wn-download-steps"><span className="done"><ShieldCheck size={13} /> Download</span><span className="active"><Layers3 size={13} /> Assemble</span><span><Zap size={13} /> Finalize</span></div>
      <div className="wn-download-graph">{[32, 48, 44, 68, 72, 55, 82, 76, 88, 70, 92, 84].map((height, index) => <i key={index} style={{ height: `${height}%` }} />)}</div>
    </div>
  )
}

function LauncherHeroMock() {
  return (
    <div className="wn-launcher-mock" aria-hidden="true">
      <div className="wn-mock-titlebar"><span>0XOLEMON</span><i /><i /><i /></div>
      <div className="wn-mock-shell">
        <aside><b /><span /><span /><span /><span /><span /></aside>
        <main>
          <div className="wn-mock-hero"><div><small>FEATURED</small><strong>Frontiers of Pandora</strong><span>Continue where you left off.</span></div></div>
          <div className="wn-mock-stats"><i /><i /><i /></div>
          <div className="wn-mock-cards"><i /><i /><i /><i /></div>
        </main>
        <div className="wn-mock-social"><UsersRound size={14} />{['AK', 'MO', 'KA'].map((item) => <i key={item}>{item}</i>)}</div>
      </div>
    </div>
  )
}

function ReleaseHistory({ compact = false, limit }: { compact?: boolean; limit?: number }) {
  const { t } = useLocale()
  const releases = typeof limit === 'number' ? CHANGELOG.slice(0, limit) : CHANGELOG
  return (
    <div className={`whats-new-release-list${compact ? ' is-compact' : ''}`}>
      {releases.map((release, index) => (
        <article key={release.version} className={`whats-new-release-card${index === 0 ? ' is-latest' : ''}`}>
          <header>
            <div className="whats-new-release-version">
              <span><Tag size={16} /></span>
              <strong>{t.whatsNew.version} {release.version}</strong>
              {index === 0 ? <i>{t.whatsNew.latest}</i> : null}
            </div>
            <time><Calendar size={14} />{release.date}</time>
          </header>
          <ul>{release.changes.map((change, itemIndex) => <li key={`${release.version}-${itemIndex}`}>{change}</li>)}</ul>
        </article>
      ))}
    </div>
  )
}

type ScrollRootRef = RefObject<HTMLElement | null>

function Reveal({
  children,
  className,
  root,
  reducedMotion,
  delay = 0,
}: {
  children: ReactNode
  className?: string
  root: ScrollRootRef
  reducedMotion: boolean
  delay?: number
}) {
  const ref = useRef<HTMLDivElement>(null)
  const { scrollYProgress } = useScroll({
    container: root,
    target: ref,
    offset: ['start 94%', 'end 16%'],
  })
  const smoothReveal = useSpring(scrollYProgress, { stiffness: 150, damping: 30, restDelta: .001 })
  const revealStart = Math.min(.28, .14 + delay)
  const revealOpacity = useTransform(smoothReveal, [0, revealStart, .78, 1], [.12, 1, 1, 1])
  const revealY = useTransform(smoothReveal, [0, revealStart, .78, 1], [52, 0, 0, -8])
  const revealScale = useTransform(smoothReveal, [0, revealStart, .78, 1], [.982, 1, 1, 1])

  return (
    <motion.div
      ref={ref}
      className={className}
      initial={false}
      style={reducedMotion ? undefined : {
        opacity: revealOpacity,
        y: revealY,
        scale: revealScale,
      }}
    >
      {children}
    </motion.div>
  )
}

function CinematicPicture({ media, alt, priority }: { media: ShowcaseMedia; alt: string; priority: boolean }) {
  return (
    <picture className="wn-cinematic-picture">
      <source type="image/avif" media="(min-width: 1400px)" srcSet={media.avif1920} />
      <source type="image/avif" srcSet={media.avif1280} />
      <source type="image/webp" media="(min-width: 1400px)" srcSet={media.webp1920} />
      <img
        src={media.webp1280}
        alt={alt}
        loading={priority ? 'eager' : 'lazy'}
        fetchPriority={priority ? 'high' : 'low'}
        decoding="async"
        draggable={false}
      />
    </picture>
  )
}

function FeatureStoryChapter({
  feature,
  index,
  activeIndex,
  pageRef,
  locale,
  reducedMotion,
  onActive,
}: {
  feature: ProductFeature
  index: number
  activeIndex: number
  pageRef: ScrollRootRef
  locale: 'en-US' | 'vi-VN'
  reducedMotion: boolean
  onActive: (index: number) => void
}) {
  const chapterRef = useRef<HTMLElement>(null)
  // A chapter is taller than the viewport, so a target-ratio threshold can never
  // intersect the narrow activation band. Track whichever chapter crosses the
  // viewport center instead; this changes state only at chapter boundaries.
  const isActive = useInView(chapterRef, { root: pageRef, amount: 'some', margin: '-48% 0px -48% 0px' })
  const copy = locale === 'vi-VN' ? feature.vi : feature.en
  const Icon = feature.icon
  const media = SHOWCASE_MEDIA[feature.visual]
  const shouldMountMedia = reducedMotion || Math.abs(index - activeIndex) <= 1
  const { scrollYProgress } = useScroll({
    container: pageRef,
    target: chapterRef,
    offset: ['start end', 'end start'],
  })
  const progress = useSpring(scrollYProgress, { stiffness: 120, damping: 28, mass: .32, restDelta: .001 })
  const stageOpacity = useTransform(progress, [0, .08, .9, 1], [0, 1, 1, 0])
  const mediaScale = useTransform(progress, [0, .24, .72, 1], [1.12, 1, 1, 1.055])
  const mediaY = useTransform(progress, [0, .24, .74, 1], [56, 0, 0, -42])
  const mediaMask = useTransform(progress, [0, .2, .74, 1], [
    'inset(12% 8% 12% 8% round 28px)',
    'inset(0% 0% 0% 0% round 0px)',
    'inset(0% 0% 0% 0% round 0px)',
    'inset(9% 6% 9% 6% round 24px)',
  ])
  const copyOpacity = useTransform(progress, [0, .16, .32, .72, .88, 1], [0, 0, 1, 1, 0, 0])
  const copyY = useTransform(progress, [0, .18, .34, .72, .9, 1], [72, 54, 0, 0, -48, -70])
  const copyX = useTransform(progress, [0, .28, .72, 1], [index % 2 === 0 ? -26 : 26, 0, 0, index % 2 === 0 ? 18 : -18])
  const overlayOpacity = useTransform(progress, [0, .24, .42, .68, .88, 1], [0, 0, .92, .92, 0, 0])
  const overlayScale = useTransform(progress, [0, .34, .7, 1], [.94, 1, 1, 1.025])

  useEffect(() => {
    if (isActive) onActive(index)
  }, [index, isActive, onActive])

  return (
    <section
      ref={chapterRef}
      className={`wn-feature-story${index % 2 ? ' is-reversed' : ''}${reducedMotion ? ' is-reduced' : ''}`}
      aria-labelledby={`wn-feature-${feature.id}`}
    >
      <motion.div className="wn-feature-story-sticky" style={reducedMotion ? undefined : { opacity: stageOpacity }}>
        <motion.div
          className="wn-feature-story-media"
          style={reducedMotion ? undefined : { scale: mediaScale, y: mediaY, clipPath: mediaMask }}
        >
          {shouldMountMedia ? (
            <CinematicPicture
              media={media}
              alt={`${copy.eyebrow}: ${copy.title}`}
              priority={index === 0}
            />
          ) : <div className="wn-cinematic-media-placeholder" aria-hidden="true" />}
          <div className="wn-feature-story-shade" aria-hidden="true" />
        </motion.div>

        <motion.div
          className="wn-feature-story-copy"
          style={reducedMotion ? undefined : { opacity: copyOpacity, x: copyX, y: copyY }}
        >
          <div className="wn-feature-story-index"><span>0{index + 1}</span><i /><b>0{PRODUCT_FEATURES.length}</b></div>
          <div className="wn-highlight-kicker"><Icon size={17} /><span>{copy.eyebrow}</span></div>
          <h3 id={`wn-feature-${feature.id}`}>{copy.title}</h3>
          <p>{copy.description}</p>
          <ul>{copy.bullets.map((bullet) => <li key={bullet}><ShieldCheck size={14} />{bullet}</li>)}</ul>
        </motion.div>

        <motion.div
          className="wn-feature-story-ui"
          style={reducedMotion ? undefined : { opacity: overlayOpacity, scale: overlayScale }}
        >
          <ProductShowcaseVisual type={feature.visual} reducedMotion={reducedMotion} />
        </motion.div>

        <motion.i className="wn-feature-story-progress" style={reducedMotion ? undefined : { scaleX: progress }} aria-hidden="true" />
      </motion.div>
    </section>
  )
}

function CinematicShowcase({
  pageRef,
  locale,
  reducedMotion,
}: {
  pageRef: ScrollRootRef
  locale: 'en-US' | 'vi-VN'
  reducedMotion: boolean
}) {
  const [activeIndex, setActiveIndex] = useState(0)
  const handleActive = useCallback((index: number) => setActiveIndex(index), [])

  return (
    <div className={`wn-cinematic-showcase${reducedMotion ? ' is-reduced' : ''}`} data-active-chapter={activeIndex + 1}>
      {PRODUCT_FEATURES.map((feature, index) => (
        <FeatureStoryChapter
          key={feature.id}
          feature={feature}
          index={index}
          activeIndex={activeIndex}
          pageRef={pageRef}
          locale={locale}
          reducedMotion={reducedMotion}
          onActive={handleActive}
        />
      ))}
    </div>
  )
}

export function WhatsNewView({ isModal = false }: { isModal?: boolean }) {
  const { locale, t } = useLocale()
  const copy = PRODUCT_COPY[locale]
  const pageRef = useRef<HTMLElement>(null)
  const heroRef = useRef<HTMLElement>(null)
  const productStageRef = useRef<HTMLDivElement>(null)
  const reducedMotion = Boolean(useReducedMotion())
  const [showAllReleases, setShowAllReleases] = useState(false)
  const typedWord = useTypewriter(copy.heroWords, reducedMotion)
  const pointerX = useMotionValue(0)
  const pointerY = useMotionValue(0)
  const contourX = useSpring(useTransform(pointerX, [-1, 1], [-24, 24]), { stiffness: 120, damping: 22 })
  const contourY = useSpring(useTransform(pointerY, [-1, 1], [-18, 18]), { stiffness: 120, damping: 22 })
  const productRotateY = useSpring(useTransform(pointerX, [-1, 1], [-2.8, 2.8]), { stiffness: 130, damping: 24 })
  const productRotateX = useSpring(useTransform(pointerY, [-1, 1], [2.1, -2.1]), { stiffness: 130, damping: 24 })
  const { scrollYProgress } = useScroll({ container: pageRef, trackContentSize: true })
  const smoothProgress = useSpring(scrollYProgress, { stiffness: 120, damping: 28, restDelta: .001 })
  const heroFade = useTransform(scrollYProgress, [0, .18], [1, .88])
  const heroY = useTransform(scrollYProgress, [0, .18], [0, -54])
  const heroScale = useTransform(scrollYProgress, [0, .18], [1, .982])
  const { scrollYProgress: productScrollProgress } = useScroll({
    container: pageRef,
    target: productStageRef,
    offset: ['start 94%', 'end 18%'],
  })
  const smoothProductProgress = useSpring(productScrollProgress, { stiffness: 135, damping: 28, restDelta: .001 })
  const productParallaxY = useTransform(smoothProductProgress, [0, 1], [34, -22])
  const productParallaxScale = useTransform(smoothProductProgress, [0, .45, 1], [.97, 1, 1.012])

  const productCountLabel = useMemo(() => PRODUCT_CATALOG.length.toString().padStart(2, '0'), [])

  if (isModal) {
    return (
      <section className="whats-new-modal-history" aria-label={t.whatsNew.title}>
        <ReleaseHistory compact />
      </section>
    )
  }

  const latest = CHANGELOG[0]
  const scrollTo = (id: string) => document.getElementById(id)?.scrollIntoView({ behavior: reducedMotion ? 'auto' : 'smooth', block: 'start' })

  const handlePointerMove = (event: ReactPointerEvent<HTMLElement>) => {
    if (reducedMotion) return
    const rect = event.currentTarget.getBoundingClientRect()
    const normalizedX = ((event.clientX - rect.left) / Math.max(1, rect.width)) * 2 - 1
    const normalizedY = ((event.clientY - rect.top) / Math.max(1, rect.height)) * 2 - 1
    pointerX.set(Math.max(-1, Math.min(1, normalizedX)))
    pointerY.set(Math.max(-1, Math.min(1, normalizedY)))
    event.currentTarget.style.setProperty('--wn-pointer-x', `${((normalizedX + 1) / 2) * 100}%`)
    event.currentTarget.style.setProperty('--wn-pointer-y', `${((normalizedY + 1) / 2) * 100}%`)
  }

  const resetPointer = () => {
    pointerX.set(0)
    pointerY.set(0)
  }

  return (
    <section
      ref={pageRef}
      className="whats-new-product-page"
      onPointerMove={handlePointerMove}
      onPointerLeave={resetPointer}
    >
      <div className="whats-new-scroll-progress" aria-hidden="true"><motion.i style={{ scaleX: smoothProgress }} /></div>

      <section ref={heroRef} className="wn-hero" aria-label="0xoLemon product introduction">
        <motion.div className="wn-contour-field" style={{ x: contourX, y: contourY }} aria-hidden="true"><ContourField /></motion.div>
        <div className="wn-pointer-light" aria-hidden="true" />
        <motion.div className="wn-hero-inner" style={reducedMotion ? undefined : { opacity: heroFade, y: heroY, scale: heroScale }}>
          <button type="button" className="wn-version-chip" onClick={() => scrollTo('release-history')}>
            <Sparkles size={14} />
            <span>{copy.versionLabel} {latest?.version ?? '2.0.50'}</span>
            <ArrowRight size={14} />
          </button>
          <h1>
            <span>0xoLemon</span>
            <strong>{copy.heroPrefix} <em>{typedWord}</em><i aria-hidden="true" /></strong>
          </h1>
          <p>{copy.heroSubtitle}</p>
          <div className="wn-hero-actions">
            <button type="button" className="wn-round-action primary" onClick={() => scrollTo('products')}><ArrowRight size={19} /><span>{copy.explore}</span></button>
            <button type="button" className="wn-round-action" onClick={() => scrollTo('release-history')}><Tag size={18} /><span>{copy.releases}</span></button>
          </div>
          <div className="wn-hero-foot"><Radio size={13} /><span>{copy.status}</span><b>{productCountLabel} PRODUCT</b></div>
        </motion.div>
      </section>

      <section id="products" className="wn-products-section">
        <Reveal className="wn-section-heading" root={pageRef} reducedMotion={reducedMotion}>
          <span>{copy.productKicker}</span>
          <h2>{copy.productTitle}</h2>
          <p>{copy.productSubtitle}</p>
        </Reveal>

        <div className="wn-products-gallery">
          {PRODUCT_CATALOG.map((product) => (
            <Reveal key={product.id} className="wn-product-reveal" root={pageRef} reducedMotion={reducedMotion} delay={0.04}>
              <article className="wn-product-exhibit">
                <div className="wn-product-exhibit-copy">
                  <div className="wn-product-index"><span>{product.index}</span><i /> <b>{product.status}</b></div>
                  <h3>{product.name}</h3>
                  <p>{locale === 'vi-VN' ? product.summary.vi : product.summary.en}</p>
                  <div className="wn-product-meta"><span>{product.category}</span><span>{latest?.version ?? '2.0.50'}</span></div>
                  <button type="button" onClick={() => scrollTo('launcher-highlights')}>{copy.exploreLauncher}<ArrowRight size={16} /></button>
                </div>
                <motion.div
                  ref={productStageRef}
                  className="wn-product-exhibit-stage"
                  style={reducedMotion ? undefined : {
                    rotateX: productRotateX,
                    rotateY: productRotateY,
                    y: productParallaxY,
                    scale: productParallaxScale,
                  }}
                >
                  <LauncherHeroMock />
                </motion.div>
              </article>
            </Reveal>
          ))}
        </div>
      </section>

      <section id="launcher-highlights" className="wn-highlights-section">
        <Reveal className="wn-section-heading is-centered" root={pageRef} reducedMotion={reducedMotion}>
          <span>{copy.highlightsKicker}</span>
          <h2>{copy.highlightsTitle}</h2>
          <p>{copy.highlightsSubtitle}</p>
        </Reveal>
        <CinematicShowcase pageRef={pageRef} locale={locale} reducedMotion={reducedMotion} />
      </section>

      <section id="release-history" className="whats-new-release-section">
        <Reveal className="whats-new-release-heading" root={pageRef} reducedMotion={reducedMotion}>
          <span>{copy.releaseKicker}</span>
          <h2>{copy.releaseTitle}</h2>
          <p>{copy.releaseSubtitle}</p>
        </Reveal>
        <Reveal root={pageRef} reducedMotion={reducedMotion} delay={0.04}>
          <ReleaseHistory limit={showAllReleases ? undefined : 3} />
          {CHANGELOG.length > 3 ? (
            <button type="button" className="wn-release-toggle" onClick={() => setShowAllReleases((value) => !value)}>
              {showAllReleases ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
              {showAllReleases ? copy.showLess : copy.showAll}
            </button>
          ) : null}
        </Reveal>
      </section>
    </section>
  )
}
