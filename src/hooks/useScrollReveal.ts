/**
 * useScrollReveal
 * ---------------
 * Reveals elements inside the current workspace scroll container. Big Picture
 * temporarily unmounts the normal launcher tree, so the workspace DOM node can
 * be destroyed and recreated. This hook deliberately rebinds whenever that root
 * changes; otherwise newly remounted Settings groups stay at opacity: 0 forever.
 */
import { useEffect } from 'react'

interface Options {
  selector?: string
  threshold?: number
  rootMargin?: string
  rootSelector?: string
}

export function useScrollReveal(options: Options = {}) {
  const {
    selector = '.reveal, .reveal-left, .reveal-scale, .reveal-clip, .settings-section, .heading-underline',
    threshold = 0.06,
    rootMargin = '0px 0px -30px 0px',
    rootSelector = '.workspace',
  } = options

  useEffect(() => {
    let currentRoot: Element | null = null
    let intersection: IntersectionObserver | null = null
    let rootMutation: MutationObserver | null = null

    const observeAll = () => {
      if (!intersection) return
      document.querySelectorAll<HTMLElement>(selector).forEach((element) => {
        if (!element.classList.contains('is-visible')) intersection?.observe(element)
      })
    }

    const bindCurrentRoot = () => {
      const nextRoot = document.querySelector(rootSelector)
      if (nextRoot === currentRoot && intersection) {
        observeAll()
        return
      }

      intersection?.disconnect()
      rootMutation?.disconnect()
      currentRoot = nextRoot

      intersection = new IntersectionObserver(
        (entries) => {
          entries.forEach((entry) => {
            if (!entry.isIntersecting) return
            entry.target.classList.add('is-visible')
            intersection?.unobserve(entry.target)
          })
        },
        { root: currentRoot, threshold, rootMargin },
      )

      observeAll()

      if (currentRoot) {
        rootMutation = new MutationObserver(observeAll)
        rootMutation.observe(currentRoot, { childList: true, subtree: true })
      }
    }

    bindCurrentRoot()

    // Observe the document shell too. Big Picture swaps the launcher subtree, so
    // the old .workspace can disappear completely and a new one can later mount.
    const documentMutation = new MutationObserver(bindCurrentRoot)
    documentMutation.observe(document.body, { childList: true, subtree: true })

    return () => {
      intersection?.disconnect()
      rootMutation?.disconnect()
      documentMutation.disconnect()
    }
  }, [selector, threshold, rootMargin, rootSelector])
}
