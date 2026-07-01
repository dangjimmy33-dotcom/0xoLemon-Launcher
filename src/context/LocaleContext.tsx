import { createContext, useContext, useState, useEffect, type ReactNode } from 'react'
import { enUS } from '../i18n/en-US'
import { viVN } from '../i18n/vi-VN'

export type Locale = 'en-US' | 'vi-VN'

type Messages = typeof enUS

const LOCALES: Record<Locale, Messages> = {
  'en-US': enUS,
  'vi-VN': viVN as unknown as Messages,
}

const STORAGE_KEY = '0xo_locale'

interface LocaleContextValue {
  locale: Locale
  setLocale: (locale: Locale) => void
  t: Messages
}

const LocaleContext = createContext<LocaleContextValue>({
  locale: 'en-US',
  setLocale: () => {},
  t: enUS,
})

export function LocaleProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(() => {
    const saved = localStorage.getItem(STORAGE_KEY)
    return (saved === 'vi-VN' ? 'vi-VN' : 'en-US') as Locale
  })

  const setLocale = (next: Locale) => {
    setLocaleState(next)
    localStorage.setItem(STORAGE_KEY, next)
  }

  useEffect(() => {
    document.documentElement.setAttribute('data-locale', locale)
  }, [locale])

  return (
    <LocaleContext.Provider value={{ locale, setLocale, t: LOCALES[locale] }}>
      {children}
    </LocaleContext.Provider>
  )
}

export function useLocale() {
  return useContext(LocaleContext)
}
