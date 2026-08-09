import { createContext, useContext } from 'react'
import { enUS } from '../i18n/en-US'
import { viVN } from '../i18n/vi-VN'

export type Locale = 'en-US' | 'vi-VN'
export type Messages = typeof enUS

export const LOCALES: Record<Locale, Messages> = {
  'en-US': enUS,
  'vi-VN': viVN as unknown as Messages,
}

export const LOCALE_STORAGE_KEY = '0xo_locale'

export interface LocaleContextValue {
  locale: Locale
  setLocale: (locale: Locale) => void
  t: Messages
}

export const LocaleContext = createContext<LocaleContextValue>({
  locale: 'en-US',
  setLocale: () => undefined,
  t: enUS,
})

export function useLocale() {
  return useContext(LocaleContext)
}
