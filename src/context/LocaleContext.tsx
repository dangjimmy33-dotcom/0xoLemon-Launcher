import { useState, useEffect, type ReactNode } from 'react'
import { LOCALES, LOCALE_STORAGE_KEY, LocaleContext, type Locale } from './locale'

export function LocaleProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(() => {
    const saved = localStorage.getItem(LOCALE_STORAGE_KEY)
    return (saved === 'vi-VN' ? 'vi-VN' : 'en-US') as Locale
  })

  const setLocale = (next: Locale) => {
    setLocaleState(next)
    localStorage.setItem(LOCALE_STORAGE_KEY, next)
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
