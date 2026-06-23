/**
 * useScrollReveal
 * ---------------
 * Watches `.reveal`, `.reveal-left`, `.reveal-scale`, and `.settings-group`
 * elements inside a given root (defaults to the `.workspace` scroll container).
 * When they enter the viewport it adds the `is-visible` class which triggers
 * the CSS transition defined in premium.css (Google Antigravity style).
 */
import { useEffect } from 'react'

interface Options {
  /** CSS selector for elements to observe. Default covers all reveal classes + settings groups */
  selector?: string
  /** IntersectionObserver threshold. Default 0.08 */
  threshold?: number
  /** Root margin. Default '0px 0px -40px 0px' (trigger 40px before the bottom edge) */
  rootMargin?: string
  /** Scroll container selector. Default '.workspace' */
  rootSelector?: string
}

export function useScrollReveal(options: Options = {}) {
  const {
    selector = '.reveal, .reveal-left, .reveal-scale, .reveal-clip, .settings-group, .settings-section, .heading-underline',
    threshold = 0.06,
    rootMargin = '0px 0px -30px 0px',
    rootSelector = '.workspace',
  } = options

  useEffect(() => {
    const root = document.querySelector(rootSelector) ?? null

    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) {
            entry.target.classList.add('is-visible')
            observer.unobserve(entry.target)
          }
        })
      },
      { root, threshold, rootMargin }
    )

    function observeAll() {
      document.querySelectorAll<HTMLElement>(selector).forEach((el) => {
        if (!el.classList.contains('is-visible')) {
          observer.observe(el)
        }
      })
    }

    observeAll()

    const mutation = new MutationObserver(observeAll)
    const workspace = document.querySelector(rootSelector)
    if (workspace) {
      mutation.observe(workspace, { childList: true, subtree: true })
    }

    return () => {
      observer.disconnect()
      mutation.disconnect()
    }
  }, [selector, threshold, rootMargin, rootSelector])
}
